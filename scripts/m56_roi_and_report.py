#!/usr/bin/env python3
"""GL-CA-03: ROI summary and cluster analysis report for M#56."""

from __future__ import annotations

import datetime as dt
import json
from pathlib import Path

from m56_common import (
    BENCHMARKS_DIR,
    TMP_CONTEXT_PATH,
    load_tier_map,
    minor_deviation_docs,
    parse_args,
    read_csv_rows,
    run_id,
)


RAW_PATH = BENCHMARKS_DIR / "m56_pixel_energy_raw.csv"
ASSIGN_PATH = BENCHMARKS_DIR / "m56_category_assignments.csv"
OUTPUT_PATH = BENCHMARKS_DIR / "CLUSTER_ANALYSIS_M56.md"

COMPLEXITY_WEIGHT = {
    "CAT-3": 0.5,
    "CAT-6": 3.0,
    "CAT-7": 2.0,
    "CAT-8": 0.5,
    "CAT-9": 2.0,
    "CAT-X": 0.0,
}


def grouped_rows(path: Path, key: str) -> dict[str, list[dict[str, str]]]:
    rows = read_csv_rows(path)
    grouped: dict[str, list[dict[str, str]]] = {}
    for row in rows:
        grouped.setdefault(row[key], []).append(row)
    return grouped


def tile_summary(rows: list[dict[str, str]]) -> str:
    ordered = sorted(rows, key=lambda row: int(row["rank"]))
    parts = [
        f"(r{row['tile_r']},c{row['tile_c']},{float(row['energy_pct']):.4f}%)"
        for row in ordered
    ]
    return "[" + ", ".join(parts) + "]"


def roi_rows(assignments: list[dict[str, str]]) -> list[dict[str, str | float | int]]:
    by_cat: dict[str, list[float]] = {cat: [] for cat in COMPLEXITY_WEIGHT}
    for row in assignments:
        by_cat[row["primary_category"]].append(float(row["ssim"]))

    result = []
    for category, weight in COMPLEXITY_WEIGHT.items():
        scores = by_cat.get(category, [])
        doc_count = len(scores)
        mean_ssim = sum(scores) / doc_count if doc_count else 1.0
        ssim_gap = 1.0 - mean_ssim if doc_count else 0.0
        roi_score = round(doc_count * ssim_gap * 100.0 * weight, 1)
        result.append(
            {
                "category": category,
                "doc_count": doc_count,
                "mean_ssim": mean_ssim,
                "ssim_gap": ssim_gap,
                "complexity_weight": weight,
                "roi_score": roi_score,
            }
        )
    result.sort(key=lambda row: (-float(row["roi_score"]), row["category"]))  # type: ignore[arg-type]
    return result


def main() -> None:
    parse_args("Compute M56 ROI table and markdown report.")
    raw_grouped = grouped_rows(RAW_PATH, "doc_name")
    assignments = sorted(read_csv_rows(ASSIGN_PATH), key=lambda row: row["doc_name"])
    baseline_ssim = {row["doc_name"]: row["ssim"] for row in minor_deviation_docs()}
    tier_map = load_tier_map()
    context = json.loads(TMP_CONTEXT_PATH.read_text()) if TMP_CONTEXT_PATH.exists() else {}

    roi = roi_rows(assignments)
    today = dt.date.today().isoformat()
    report_run_id = run_id()

    lines: list[str] = []
    lines.append(f"# CLUSTER_ANALYSIS_M56 — {today} — run_id: {report_run_id}")
    lines.append("")
    lines.append("## ROI Summary Table")
    lines.append("")
    lines.append("| category | doc_count | mean_ssim | ssim_gap | complexity_weight | roi_score |")
    lines.append("|---|---:|---:|---:|---:|---:|")
    for row in roi:
        lines.append(
            "| {category} | {doc_count} | {mean_ssim:.4f} | {ssim_gap:.4f} | {complexity_weight:.1f} | {roi_score:.1f} |".format(
                **row
            )
        )

    lines.append("")
    lines.append("## Per-Document Analysis")
    lines.append("")

    for row in assignments:
        doc_name = row["doc_name"]
        raw_rows = sorted(raw_grouped[doc_name], key=lambda item: int(item["rank"]))
        top3_combined = sum(float(item["energy_pct"]) for item in raw_rows)
        ctx = context.get(doc_name, {})
        features = ctx.get("features", {})
        lines.append(f"### {doc_name}")
        lines.append(
            "- "
            f"ssim: {float(baseline_ssim[doc_name]):.6f}, "
            f"tier: {tier_map.get(doc_name, '')}, "
            f"top_3_tiles: {tile_summary(raw_rows)}, "
            f"top_3_combined_pct: {top3_combined:.4f}, "
            f"dominant_tile_type: {row['dominant_tile_type']}, "
            f"has_checkbox: {row['has_checkbox']}, "
            f"has_draw_text_positioned: {row['has_draw_text_positioned']}, "
            f"has_table: {row['has_table']}, "
            f"has_border_lines: {row['has_border_lines']}, "
            f"primary_category: {row['primary_category']}, "
            f"confidence: {row['confidence']}, "
            f"category_rationale: {ctx.get('category_rationale', 'Decision rule 7 triggered: dominant tile type did not match any category rule.')}"
        )
        lines.append("")

    OUTPUT_PATH.write_text("\n".join(lines).rstrip() + "\n")
    print(json.dumps({"output": str(OUTPUT_PATH), "run_id": report_run_id, "roi_rows": len(roi)}))


if __name__ == "__main__":
    main()
