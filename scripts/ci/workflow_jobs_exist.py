#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A `needs:` or a `runs-on:` may not name a job that is not there.

Removing a job is easy; removing every reference to it is what gets missed.
GitHub refuses the whole file for a dangling reference and reports it as "this
run likely failed because of a workflow file issue" -- the same message as a
syntax error, next to the workflow's *path* instead of its name, which is the
only visible clue that the file was never read.

That happened while `create-runner` jobs were being taken back out of five
workflows on 28-08-2026. Every YAML parser accepts a `needs:` pointing at
nothing; only GitHub minds (#274).

# FLOOR: workflows read >= 10 — the repository carries around 30, and a glob
# that finds nothing sees no dangling references either.
"""

from __future__ import annotations

import pathlib
import re
import sys

import yaml

MINIMUM_WORKFLOWS = 10
FLOWS = pathlib.Path(".github/workflows")
NEEDS_IN_EXPR = re.compile(r"needs\.([\w-]+)\.")


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[jobs-exist] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    gelezen, treffers = 0, []

    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as fout:
            print(f"[jobs-exist] FATAL: {pad.name} does not parse: {fout}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict):
            continue
        gelezen += 1
        jobs = doc.get("jobs") or {}
        for naam, job in jobs.items():
            if not isinstance(job, dict):
                continue
            needs = job.get("needs")
            if isinstance(needs, str):
                needs = [needs]
            for n in needs or []:
                if n not in jobs:
                    treffers.append((pad.name, naam, f"needs: {n}"))
            # `runs-on: ${{ needs.create-runner.outputs.label }}` and any other
            # expression reaching into a job that is gone.
            for veld in ("runs-on", "if"):
                for m in NEEDS_IN_EXPR.finditer(str(job.get(veld, ""))):
                    if m.group(1) not in jobs:
                        treffers.append((pad.name, naam, f"{veld}: needs.{m.group(1)}"))

    if gelezen < MINIMUM_WORKFLOWS:  # FLOOR
        print(f"[jobs-exist] FATAL: {gelezen} workflow(s) read, floor is "
              f"{MINIMUM_WORKFLOWS}. A glob that finds nothing sees no dangling "
              "references either.", file=sys.stderr)
        return 1

    if treffers:
        print(f"[jobs-exist] FATAL: {len(treffers)} reference(s) to a job that is not "
              "there:", file=sys.stderr)
        for workflow, job, wat in treffers:
            print(f"  {workflow} :: {job} :: {wat}", file=sys.stderr)
        print("\nGitHub refuses the whole file for this and reports it the same way it "
              "reports a syntax error, beside the workflow's path instead of its name.",
              file=sys.stderr)
        return 1

    print(f"[jobs-exist] OK: {gelezen} workflow(s); every needs and runs-on names a job "
          "that exists.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
