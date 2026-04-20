#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
from collections import Counter, defaultdict
from datetime import date
from pathlib import Path


CLASSIFICATIONS = ["INVERTED", "MISSING", "EXTRA", "WRONG", "MATCH", "UNKNOWN"]


def load_tiers(tiers_csv: Path) -> list[dict[str, str]]:
    with tiers_csv.open() as handle:
        return list(csv.DictReader(handle))


def load_raw(raw_csv: Path) -> list[dict[str, str]]:
    with raw_csv.open() as handle:
        return list(csv.DictReader(handle))


def load_ssim_map(baseline_json: Path) -> dict[str, float]:
    payload = json.loads(baseline_json.read_text())
    return {row["file"]: float(row["ssim"]) for row in payload["results"]}


def pct(value: int, total: int) -> str:
    return f"{(100.0 * value / total) if total else 0.0:.1f}%"


def build_report(
    tiers: list[dict[str, str]],
    raw_rows: list[dict[str, str]],
    ssim_map: dict[str, float],
    report_date: str,
) -> str:
    total_docs = len(tiers)
    rows_by_doc: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in raw_rows:
        rows_by_doc[row["doc_name"]].append(row)

    tier_ranges = {1: "SSIM <= 0.910", 2: "0.910 < SSIM <= 0.940", 3: "SSIM > 0.940"}
    coverage: dict[int, dict[str, int]] = {1: {"docs": 0, "fields": 0}, 2: {"docs": 0, "fields": 0}, 3: {"docs": 0, "fields": 0}}
    for tier_row in tiers:
        coverage[int(tier_row["tier"])]["docs"] += 1
    for row in raw_rows:
        coverage[int(row["tier"])]["fields"] += 1

    field_counts = Counter(row["classification"] for row in raw_rows)
    doc_sets = {name: {row["classification"] for row in rows} for name, rows in rows_by_doc.items()}
    doc_counts = {
        "INVERTED": sum("INVERTED" in states for states in doc_sets.values()),
        "MISSING": sum("MISSING" in states for states in doc_sets.values()),
        "EXTRA": sum("EXTRA" in states for states in doc_sets.values()),
        "WRONG": sum("WRONG" in states for states in doc_sets.values()),
        "MATCH": sum(states == {"MATCH"} for states in doc_sets.values()),
        "UNKNOWN": sum("UNKNOWN" in states for states in doc_sets.values()),
    }
    docs_with_issue = sum(bool(states & {"INVERTED", "MISSING", "EXTRA", "WRONG"}) for states in doc_sets.values())

    lines: list[str] = []
    lines.append(f"# DATA_FIDELITY_BROAD_M56 — {report_date}")
    lines.append("")
    lines.append("| tier | ssim range | docs in tier | docs audited | fields audited |")
    lines.append("| --- | --- | ---: | ---: | ---: |")
    for tier in (1, 2, 3):
        lines.append(
            f"| {tier} | {tier_ranges[tier]} | {coverage[tier]['docs']} | {coverage[tier]['docs']} | {coverage[tier]['fields']} |"
        )
    lines.append("")
    lines.append("| classification | field count | doc count | % of audited docs |")
    lines.append("| --- | ---: | ---: | ---: |")
    for classification in CLASSIFICATIONS:
        lines.append(
            f"| {classification} | {field_counts.get(classification, 0)} | {doc_counts[classification]} | {pct(doc_counts[classification], total_docs)} |"
        )
    lines.append("")
    lines.append(f"Docs with ≥1 fidelity issue: {docs_with_issue} / {total_docs} ({pct(docs_with_issue, total_docs)})")
    lines.append("")
    lines.append("## Conclusion")
    lines.append(
        f"Off→On is broader than 3 known docs: {doc_counts['INVERTED']} docs show at least one INVERTED classification."
    )
    lines.append(
        f"On→Off pattern is present in {doc_counts['MISSING']} docs with at least one MISSING classification."
    )
    lines.append(
        f"Text mismatches are present in {doc_counts['WRONG']} docs with at least one WRONG classification."
    )
    issue_classes = ["INVERTED", "MISSING", "EXTRA", "WRONG"]
    dominant_issue = max(issue_classes, key=lambda key: field_counts.get(key, 0))
    lines.append(f"The dominant fidelity failure mode is {dominant_issue}.")
    if doc_counts["INVERTED"] > 3:
        recommendation = f"Expand fidelity scope in M#57: {doc_counts['INVERTED']} docs show Off->On/INVERTED pattern."
    elif docs_with_issue == 0:
        recommendation = "No new fidelity issues found beyond already-fixed Off->On pattern."
    else:
        recommendation = "Fidelity scope remains focused on checkboxes: no new patterns found."
    lines.append(recommendation)
    lines.append("")
    lines.append("## Per-Document Results")
    for tier_row in tiers:
        doc_name = tier_row["doc_name"]
        tier = int(tier_row["tier"])
        ssim = ssim_map[doc_name]
        lines.append("")
        lines.append(f"### {doc_name} (Tier {tier}, SSIM {ssim:.4f})")
        lines.append("| field | type | xfa_value | output_state | oracle_state | classification |")
        lines.append("| --- | --- | --- | --- | --- | --- |")
        for row in rows_by_doc[doc_name]:
            lines.append(
                f"| {row['field_name']} | {row['field_type']} | {row['xfa_value']} | "
                f"{row['output_state']} | {row['oracle_state']} | {row['classification']} |"
            )
    lines.append("")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="M#56 fidelity report builder")
    parser.add_argument("--tiers-csv", default="benchmarks/m56_fidelity_tiers.csv")
    parser.add_argument("--raw-csv", default="benchmarks/m56_fidelity_raw.csv")
    parser.add_argument("--baseline-json", default="benchmarks/enterprise-baseline-ENT05-final.json")
    parser.add_argument("--output", default="benchmarks/DATA_FIDELITY_BROAD_M56.md")
    parser.add_argument("--date", default=str(date.today()))
    args = parser.parse_args()

    tiers = load_tiers(Path(args.tiers_csv))
    raw_rows = load_raw(Path(args.raw_csv))
    ssim_map = load_ssim_map(Path(args.baseline_json))
    report = build_report(tiers, raw_rows, ssim_map, args.date)

    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(report)
    print(f"Wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
