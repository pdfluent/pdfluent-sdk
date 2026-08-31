#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A guard job must run all of its guards, not stop at the first one that fails.

`orchestration-guard` had 37 steps in sequence. Step four failed, and had failed
on all thirty of the preceding runs on master -- twenty-one failures, nine
cancellations, no successes. The thirty-three steps below it never executed:
the licence gate, the fork-drift gates, the capability matrix, the
shell-injection guard, PR staleness, and about twenty more. Every one of them
read as an installed control while measuring nothing, and one of them was
discovered only because an agent added a guard, watched it go green, and then
found it had never run.

A job that stops at the first failure reports one problem and hides the rest, so
each fix reveals the next one a day later. Worse, a step that never ran looks
the same in the summary as a step that passed.

The remedy is one line per step: `if: ${{ !cancelled() }}`. Every guard still
runs, every failure is still a failure, and the job still goes red -- you simply
see all of them at once.

This check keeps that true for steps added later, which is the part a convention
cannot do on its own.

Exit codes:
  0  every guard step runs regardless of its predecessors
  1  a guard step would be skipped when an earlier one fails
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("SKIPPED (not a pass): pyyaml is not installed, so the workflows cannot be read", file=sys.stderr)
    sys.exit(3)

ROOT = Path(__file__).resolve().parents[2]

# Jobs whose purpose is to run a series of independent checks. A failure in one
# says nothing about whether the next one would pass, so the next one must run.
GUARD_JOBS = {
    ("ci.yml", "orchestration-guard"),
}

# Steps that genuinely are prerequisites: if the checkout or the interpreter is
# missing, every step below is meaningless rather than unmeasured. Aborting
# there is correct, so they are exempt by name.
PREREQUISITE_MARKERS = (
    "actions/checkout",
    "actions/setup-python",
    "actions/setup-node",
    "pip install",
    "npm ci",
)

RUNS_ANYWAY = ("!cancelled()", "always()", "success() || failure()")


def is_prerequisite(step: dict) -> bool:
    blob = f"{step.get('uses', '')} {step.get('run', '')} {step.get('name', '')}"
    return any(marker in blob for marker in PREREQUISITE_MARKERS)


def main() -> int:
    problems: list[str] = []
    checked = 0

    for filename, job_name in sorted(GUARD_JOBS):
        path = ROOT / ".github" / "workflows" / filename
        if not path.exists():
            print(f"SKIPPED (not a pass): {path} is missing", file=sys.stderr)
            return 3
        workflow = yaml.safe_load(path.read_text())
        job = (workflow.get("jobs") or {}).get(job_name)
        if job is None:
            problems.append(f"{filename}: job `{job_name}` no longer exists. Update GUARD_JOBS or restore it.")
            continue

        steps = job.get("steps") or []
        seen_a_guard = False
        for index, step in enumerate(steps, start=1):
            if is_prerequisite(step):
                if seen_a_guard:
                    problems.append(
                        f"{filename}:{job_name} step {index} ({step.get('name') or step.get('uses')}) "
                        "looks like setup but sits below a guard, so a guard failure would skip it."
                    )
                continue
            seen_a_guard = True
            checked += 1
            condition = str(step.get("if", ""))
            if not any(marker in condition for marker in RUNS_ANYWAY):
                label = step.get("name") or str(step.get("run", ""))[:60]
                problems.append(
                    f"{filename}:{job_name} step {index} ({label}) has no `if:` that survives an "
                    "earlier failure. Add `if: ${{ !cancelled() }}` so it still runs and still fails."
                )

    if checked == 0 and not problems:
        print(
            "SKIPPED (not a pass): no guard steps were examined. Either GUARD_JOBS is empty\n"
            "  or every step matched a prerequisite marker -- both mean this check measured nothing.",
            file=sys.stderr,
        )
        return 3

    if problems:
        print(f"Guard steps that hide behind an earlier failure: {len(problems)}\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    print(f"✓ {checked} guard step(s) across {len(GUARD_JOBS)} job(s) run regardless of their predecessors")
    return 0


if __name__ == "__main__":
    sys.exit(main())
