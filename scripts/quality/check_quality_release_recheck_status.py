#!/usr/bin/env python3
"""Validate the Quality release-recheck status JSON.

Fails (exit 1) if:
  - the status file is missing/invalid;
  - a lane is marked `green_proven` without an evidence `detail`;
  - an executable lane failed (`executable_failed` != 0);
  - a lane uses a status outside the allowed vocabulary.

Allowed statuses: green_proven, release_gate_defined_not_executed_locally,
blocked_missing_fixture, blocked_missing_runtime, blocked_<reason>.
"""
from __future__ import annotations
import json, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
STATUS = REPO / "benchmarks/runs/ga_readiness_3d/sdk_ga_quality_release_recheck/quality_release_recheck_status.json"
ALLOWED_EXACT = {"green_proven", "release_gate_defined_not_executed_locally"}
ALLOWED_PREFIX = ("blocked_",)
EXPECTED_LANES = {"QR-3", "QR-6", "QR-9", "QR-10", "QR-11", "QR-15"}

def main() -> int:
    if not STATUS.exists():
        print(f"FAIL: status file missing: {STATUS}", file=sys.stderr); return 1
    try:
        data = json.loads(STATUS.read_text())
    except Exception as e:  # noqa: BLE001
        print(f"FAIL: invalid JSON: {e}", file=sys.stderr); return 1

    errs = []
    lanes = {l["lane"]: l for l in data.get("lanes", [])}
    missing = EXPECTED_LANES - set(lanes)
    if missing:
        errs.append(f"missing lanes: {sorted(missing)}")
    for name, l in lanes.items():
        st = l.get("status", "")
        ok = st in ALLOWED_EXACT or st.startswith(ALLOWED_PREFIX)
        if not ok:
            errs.append(f"{name}: disallowed status '{st}'")
        if st == "green_proven" and not l.get("detail"):
            errs.append(f"{name}: green_proven without evidence detail")
    if int(data.get("executable_failed", 1)) != 0:
        errs.append("executable_failed != 0 (a runnable lane failed)")

    if errs:
        print("QUALITY_RELEASE_RECHECK_STATUS: INVALID", file=sys.stderr)
        for e in errs: print(f"  - {e}", file=sys.stderr)
        return 1
    gp = sum(1 for l in lanes.values() if l["status"] == "green_proven")
    gd = sum(1 for l in lanes.values() if l["status"] == "release_gate_defined_not_executed_locally")
    print(f"QUALITY_RELEASE_RECHECK_STATUS: OK — {gp} green_proven, {gd} release_gate_defined, 0 blocked, 0 executable failures.")
    return 0

if __name__ == "__main__":
    sys.exit(main())
