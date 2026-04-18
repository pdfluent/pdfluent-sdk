#!/usr/bin/env python3
"""Archive a GATE result to benchmarks/gate-history/ for trend tracking.

Usage:
    python3 scripts/archive_gate_result.py \
        --gate-id 57c \
        --result /tmp/ssim_results_1000_gate57c.json \
        --commit d4bbe608f \
        --binary pdfluent

Creates benchmarks/gate-history/gate-{id}.json with summary fields.
"""
import argparse
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def git_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], text=True
        ).strip()
    except Exception:
        return "unknown"


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--gate-id", required=True, help="e.g. '57c' or '58'")
    p.add_argument("--result", required=True, help="Path to gate JSON result")
    p.add_argument("--commit", default=None, help="Git commit hash (defaults to HEAD)")
    p.add_argument("--binary", default="pdfluent")
    p.add_argument("--notes", default="", help="Optional notes")
    args = p.parse_args()

    result_path = Path(args.result)
    if not result_path.exists():
        raise SystemExit(f"ERROR: result file not found: {result_path}")

    data = json.loads(result_path.read_text())
    summary = data.get("summary", {})
    results = data.get("results", [])

    commit = args.commit or git_commit()

    archive_entry = {
        "gate_id": args.gate_id,
        "date": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        "commit": commit,
        "binary": args.binary,
        "corpus_size": summary.get("total", len(results)),
        "pass": summary.get("pass", 0),
        "fail": summary.get("fail", 0),
        "crash": summary.get("crash", 0),
        "oracle_fault": summary.get("oracle_fault", 0),
        "mean_ssim": summary.get("mean_ssim"),
        "min_ssim": summary.get("min_ssim"),
        "threshold": summary.get("threshold", 0.95),
        "pass_rate": None,
        "notes": args.notes,
    }

    total = archive_entry["corpus_size"]
    oracle_fault = archive_entry["oracle_fault"]
    effective = total - oracle_fault
    if effective > 0:
        archive_entry["pass_rate"] = round(archive_entry["pass"] / effective, 4)

    out_dir = Path(__file__).parent.parent / "benchmarks" / "gate-history"
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"gate-{args.gate_id}.json"
    out_path.write_text(json.dumps(archive_entry, indent=2))
    print(f"Archived: {out_path}")
    print(json.dumps(archive_entry, indent=2))


if __name__ == "__main__":
    main()
