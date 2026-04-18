#!/usr/bin/env python3
"""Update corpus/CI_BASELINE.json from a new GATE result.

Usage:
    python3 scripts/update_ci_baseline.py \
        --result /tmp/gate_ci_result.json \
        --out corpus/CI_BASELINE.json
"""
import argparse
import json
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--result", required=True)
    p.add_argument("--out", default="corpus/CI_BASELINE.json")
    args = p.parse_args()

    data = json.loads(Path(args.result).read_text())
    summary = data.get("summary", {})
    results = data.get("results", [])

    total = summary.get("total", len(results))
    oracle_fault = summary.get("oracle_fault", 0)
    effective = total - oracle_fault
    pass_count = summary.get("pass", 0)
    pass_rate = round(pass_count / effective, 4) if effective > 0 else 0.0

    pass_files = [
        r["file"] for r in results
        if r.get("render", {}).get("status") == "pass"
    ]

    baseline = {
        "pass_rate": pass_rate,
        "pass": pass_count,
        "fail": summary.get("fail", 0),
        "crash": summary.get("crash", 0),
        "oracle_fault": oracle_fault,
        "total": total,
        "mean_ssim": summary.get("mean_ssim"),
        "pass_files": pass_files,
    }

    out = Path(args.out)
    out.write_text(json.dumps(baseline, indent=2))
    print(f"Updated {out}: pass_rate={pass_rate*100:.2f}% ({pass_count}/{effective})")


if __name__ == "__main__":
    main()
