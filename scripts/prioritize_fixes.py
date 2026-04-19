#!/usr/bin/env python3
"""
prioritize_fixes.py — Assign prioritization tiers to failure codes from impact scores.

Tiers (from FIX_PRIORITIZATION.md):
  must_fix         — impact_score >= 25 AND severity >= 4
  should_fix       — impact_score >= 12
  accept           — impact_score < 12
  known_limitation — manually designated architectural constraints

CLI:
    python3 scripts/prioritize_fixes.py \\
        --impact-scores benchmarks/impact_scores.json \\
        --output benchmarks/fix_priorities.json \\
        --report benchmarks/fix_priorities_report.md

Importable as module:
    from prioritize_fixes import apply_prioritization, KNOWN_LIMITATIONS
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date
from pathlib import Path

# Codes that are known architectural limitations regardless of impact score.
# These map to the "Known Limitation" tier and are excluded from sprint planning.
KNOWN_LIMITATIONS: frozenset[str] = frozenset({
    # No JS engine — FormCalc/ECMAScript scripting
    "BIND-003",
})

# Tier labels
MUST_FIX = "must_fix"
SHOULD_FIX = "should_fix"
KNOWN_LIMITATION = "known_limitation"
ACCEPT = "accept"


def assign_tier(code: str, total_score: int, severity_score: int) -> str:
    """
    Apply the prioritization decision matrix to a single failure code.

    Parameters
    ----------
    code : str
        Failure code, e.g. "BIND-001".
    total_score : int
        Computed impact score (freq × sev × enterprise).
    severity_score : int
        Severity score (1-5).

    Returns
    -------
    str
        One of: "must_fix", "should_fix", "known_limitation", "accept".
    """
    if code in KNOWN_LIMITATIONS:
        return KNOWN_LIMITATION
    if total_score >= 25 and severity_score >= 4:
        return MUST_FIX
    if total_score >= 12:
        return SHOULD_FIX
    return ACCEPT


def apply_prioritization(impact: dict) -> dict:
    """
    Assign prioritization tiers to all failure codes in an impact scores dict.

    Parameters
    ----------
    impact : dict
        Output of score_impact.py (impact_scores.json).

    Returns
    -------
    dict
        Prioritization JSON payload.
    """
    scores = impact.get("scores", [])
    total_documents = impact.get("total_documents", 0)
    total_failed = impact.get("total_failed", 0)

    priorities: list[dict] = []
    for s in scores:
        code = s["code"]
        total_score = s["total_score"]
        severity_score = s["severity_score"]
        tier = assign_tier(code, total_score, severity_score)

        priorities.append({
            "code": code,
            "count": s["count"],
            "pct": s["pct"],
            "total_score": total_score,
            "frequency_score": s["frequency_score"],
            "severity_score": severity_score,
            "enterprise_score": s["enterprise_score"],
            "avg_ssim": s["avg_ssim"],
            "tier": tier,
        })

    # Sort: must_fix first, then should_fix, then accept, then known_limitation
    tier_order = {MUST_FIX: 0, SHOULD_FIX: 1, ACCEPT: 2, KNOWN_LIMITATION: 3}
    priorities.sort(key=lambda x: (tier_order[x["tier"]], -x["total_score"]))

    summary = {
        MUST_FIX: sum(1 for p in priorities if p["tier"] == MUST_FIX),
        SHOULD_FIX: sum(1 for p in priorities if p["tier"] == SHOULD_FIX),
        ACCEPT: sum(1 for p in priorities if p["tier"] == ACCEPT),
        KNOWN_LIMITATION: sum(1 for p in priorities if p["tier"] == KNOWN_LIMITATION),
    }

    return {
        "total_documents": total_documents,
        "total_failed": total_failed,
        "summary": summary,
        "priorities": priorities,
    }


def render_report(result: dict) -> str:
    """Render a Markdown fix prioritization report."""
    today = date.today().isoformat()
    lines: list[str] = []

    total = result["total_documents"]
    total_failed = result["total_failed"]
    pct_failed = round(total_failed / total * 100, 1) if total else 0.0
    summary = result.get("summary", {})

    lines.append(f"# Fix Prioritization Report — {today}")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- **Total documents:** {total}")
    lines.append(f"- **Total failed:** {total_failed} ({pct_failed}%)")
    lines.append(f"- **Must Fix:** {summary.get(MUST_FIX, 0)} failure codes")
    lines.append(f"- **Should Fix:** {summary.get(SHOULD_FIX, 0)} failure codes")
    lines.append(f"- **Accept / no action:** {summary.get(ACCEPT, 0)} failure codes")
    lines.append(f"- **Known Limitations:** {summary.get(KNOWN_LIMITATION, 0)} failure codes")
    lines.append("")

    priorities = result.get("priorities", [])
    if not priorities:
        lines.append("_No failure data available._")
        lines.append("")
        return "\n".join(lines)

    tier_headers = {
        MUST_FIX: "Must Fix",
        SHOULD_FIX: "Should Fix",
        ACCEPT: "Accept",
        KNOWN_LIMITATION: "Known Limitation",
    }

    current_tier = None
    for p in priorities:
        tier = p["tier"]
        if tier != current_tier:
            current_tier = tier
            lines.append(f"## {tier_headers[tier]}")
            lines.append("")
            lines.append(
                "| Code | Count | % of Docs | Score | Freq | Sev | Ent | Avg SSIM |"
            )
            lines.append(
                "|------|-------|-----------|-------|------|-----|-----|---------|"
            )

        avg_ssim = f"{p['avg_ssim']:.4f}" if p["avg_ssim"] is not None else "n/a"
        lines.append(
            f"| {p['code']} | {p['count']} | {p['pct']}% "
            f"| {p['total_score']} | {p['frequency_score']} "
            f"| {p['severity_score']} | {p['enterprise_score']} | {avg_ssim} |"
        )

    lines.append("")
    lines.append(
        "_Tiers: Must Fix = score ≥ 25 AND severity ≥ 4; "
        "Should Fix = score ≥ 12; Accept = score < 12; "
        "Known Limitation = architectural constraint._"
    )
    lines.append("")

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Assign prioritization tiers to failure codes."
    )
    parser.add_argument(
        "--impact-scores",
        required=True,
        type=Path,
        help="Path to impact_scores.json",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Output path for fix_priorities.json",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=None,
        help="Output path for Markdown report (optional)",
    )
    args = parser.parse_args(argv)

    if not args.impact_scores.exists():
        print(f"ERROR: impact scores file not found: {args.impact_scores}", file=sys.stderr)
        return 1

    impact = json.loads(args.impact_scores.read_text())
    result = apply_prioritization(impact)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2))
    print(f"Fix priorities written to {args.output}")

    if args.report:
        report_md = render_report(result)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(report_md)
        print(f"Report written to {args.report}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
