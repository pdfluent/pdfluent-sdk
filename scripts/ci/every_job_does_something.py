#!/usr/bin/env python3
"""A job with no `run:` step can only be green (#292).

`measurement-guard` was called "The published measurements still measure what
they say" and, from 31-08-2026 until today, consisted of one `actions/checkout`
and nothing else. It ticked green on every pull request. Its eight guards were
not deleted -- c3b4b08b inserted a new job BETWEEN that job's checkout and its
remaining steps, so everything from `Set up Python` onward silently changed
owner. The diff was +29/-1: a pure addition, no line removed, no step text
altered. No reviewer saw it, and neither `git log -S` nor `-G` can, because the
text never changed. It was found by parsing the YAML at each commit and
counting.

That is the shape this checks for. A job that runs nothing is either a
leftover, or -- much more often -- a job whose steps have gone somewhere else
while its name stayed behind promising them.

WHAT COUNTS AS DOING SOMETHING

A `run:` step, or a `uses:` that is not merely checkout/setup. A job that only
checks out the repository and installs a language has, by construction, nothing
to report. Jobs that exist to produce an artefact or call a reusable workflow
are covered by the second half.

ALLOWED holds the deliberate exceptions, each with the reason, because a list
of names without reasons is how the next person learns to add one.
"""
from __future__ import annotations
import pathlib, sys

import yaml

WORTEL = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = WORTEL / ".github" / "workflows"

# `uses:` that do not amount to doing anything on their own.
OPZET = ("actions/checkout", "actions/setup-python", "actions/setup-node",
         "actions/setup-java", "actions/setup-dotnet", "actions/cache",
         "dtolnay/rust-toolchain", "Swatinem/rust-cache")

ALLOWED: dict[str, str] = {}


def doet_iets(job: dict) -> bool:
    for stap in (job.get("steps") or []):
        if not isinstance(stap, dict):
            continue
        if stap.get("run"):
            return True
        gebruikt = str(stap.get("uses", ""))
        if gebruikt and not gebruikt.startswith(OPZET):
            return True
    return False


def main() -> int:
    problemen: list[str] = []
    bekeken = 0

    paden = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as e:
            print(f"[job-does-something] FATAL: {pad.name} will not parse: {e}",
                  file=sys.stderr)
            return 2
        if not isinstance(doc, dict):
            continue
        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            # A job that only calls a reusable workflow has its steps there.
            if job.get("uses"):
                continue
            bekeken += 1
            sleutel = f"{pad.name}:{naam}"
            if doet_iets(job) or sleutel in ALLOWED:
                continue
            titel = job.get("name", naam)
            problemen.append(
                f"{sleutel} runs nothing: {len(job.get('steps') or [])} step(s), "
                f"none of them a command. It reports {titel!r} as a green tick "
                "and checks nothing. Either its steps went somewhere else while "
                "the name stayed, or the job is a leftover -- and the first is "
                "what happened to measurement-guard for two days.")

    if bekeken == 0:
        print("[job-does-something] FATAL: no job was read. A scan that looked "
              "at nothing cannot report agreement.", file=sys.stderr)
        return 2
    if problemen:
        print(f"[job-does-something] FAIL: {len(problemen)} job(s) do nothing:",
              file=sys.stderr)
        for p in problemen:
            print(f"    {p}", file=sys.stderr)
        return 1
    print(f"[job-does-something] OK: {bekeken} job(s); each runs at least one command")
    return 0


if __name__ == "__main__":
    sys.exit(main())
