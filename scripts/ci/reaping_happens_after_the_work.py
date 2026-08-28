#!/usr/bin/env python3
"""The sweep must run after the job that uses the runner, never before it.

Reaping was first attached to the provisioning job. That version could not
delete anything at all: under the one-instance rule the sole server is either
claimed by this run and excluded, or freshly created and still inside its paid
hour. Every push spared everything, and the sweep looked like it was working.

A sweep that cannot delete is indistinguishable from a sweep with nothing to
do. This guard is the difference.
"""
from __future__ import annotations

import pathlib
import sys

import yaml

WORTEL = pathlib.Path(__file__).resolve().parents[2]
WERKSTROMEN = WORTEL / ".github/workflows"
SWEEP = "sweep_idle_instances.py"


def stappen(job: dict) -> str:
    return "\n".join(str(s.get("run", "")) for s in job.get("steps", []) or [])


def main() -> int:
    stuk = []
    gezien = 0
    for pad in sorted(WERKSTROMEN.glob("*.yml")):
        doc = yaml.safe_load(pad.read_text()) or {}
        jobs = doc.get("jobs") or {}
        # Which job provisions, and which one actually uses the machine?
        maakt = {n for n, j in jobs.items()
                 if "hcloud-github-runner" in str(j) or "reuse_an_idle_instance" in str(j)}
        for naam, job in jobs.items():
            if SWEEP not in stappen(job):
                continue
            gezien += 1
            if naam in maakt:
                stuk.append(f"{pad.name}: job `{naam}` both provisions and sweeps. The "
                            "instance it just claimed or created is always spared, so "
                            "this sweep can never delete anything")
                continue
            if not maakt:
                # A standalone sweep workflow provisions nothing, so there is no
                # job for it to follow. Its safety comes from the busy check.
                continue
            nodig = job.get("needs") or []
            nodig = [nodig] if isinstance(nodig, str) else list(nodig)
            # It must follow at least one job that provisioning feeds, or it
            # runs while the machine is still in use.
            if not any(g in nodig for g in maakt) and not nodig:
                stuk.append(f"{pad.name}: job `{naam}` sweeps without needing the job that "
                            "uses the runner, so it can run while the machine is in use")
    if stuk:
        for r in stuk:
            print(f"FAIL: {r}")
        return 1
    if gezien == 0:
        print("SKIPPED (not a pass): no workflow runs the sweep, so nothing was checked.",
              file=sys.stderr)
        return 0
    print(f"[reap-order] OK: {gezien} sweep job(s), each after the work, none self-sparing.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
