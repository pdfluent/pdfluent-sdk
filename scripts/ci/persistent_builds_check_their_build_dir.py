#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A job that compiles on the persistent runner must look at the build directory first.

The equivalent guard for GitLab already exists -- `shared_build_dir_fails_loudly.py`
-- and GitLab is the mirror, where a red job stops nothing. GitHub is where a
merge is blocked, and on this side nothing checked. The two pipelines share one
machine, and it is that machine's build directory that keeps going away (#264).

Ephemeral runners are exempt and deliberately so. A Hetzner instance is created
for one workflow and destroyed after it, so its build directory is new, empty,
and cannot be carrying debris from a job that was killed. The persistent desktop
is the opposite: every job lands in state the previous one left behind.

WHY BEFORE THE FIRST CARGO CALL AND NOT MERELY SOMEWHERE
--------------------------------------------------------
The whole value is failing at the top of the job rather than forty minutes in.
A check that runs after the build has already started is a report, not a guard,
and on 27-08-2026 the two jobs that cost the most did not fail at all: they
stopped reporting, and were cleaned up an hour later as `no_updates_running`.

FLOOR: workflows read >= 10, and build jobs found == EXPECTED_BUILD_JOBS. A scan
that finds nothing sees no unchecked job either, and reports a clean pipeline in
exactly the same words as a scan that found nothing wrong. The second number is
pinned in both directions -- see the comment on it.

Exit codes:
  0  every persistent build job checks its build directory first
  1  one does not, or the scan lost sight of the pipeline
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

try:
    import yaml
except ImportError:
    print("SKIPPED (not a pass): pyyaml is not installed, so the workflows cannot be read", file=sys.stderr)
    sys.exit(3)

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"

# FLOOR: workflows read >= 10 — the repository carries around thirty. A glob
# that stops matching finds no unchecked job either.
MINIMUM_WORKFLOWS = 10

# FLOOR: build jobs found == 6. A TWO-WAY ratchet, and it is pinned rather than
# bounded below on purpose.
#
# Downward is the obvious danger: jobs quietly stop being recognised -- a renamed
# label, a build moved behind a composite action -- and the guard reports a clean
# pipeline because it stopped looking. A floor catches that.
#
# Upward is the one a floor misses, and it is the more likely of the two here. If
# this number climbs on its own, a lane that used to run on a throwaway instance
# has moved onto the machine that keeps its state and shares 9.4 GB of real
# headroom with everything else. That is a decision about where the risk lives,
# and it should be made rather than noticed six weeks later. Either direction:
# edit this line, and say why in the commit.
# 7 -> 8 on 04-09-2026: java-bindings.yml's `jni-tests` moved onto the desktop.
# It compiles, so it counts, and this is the upward direction the comment above
# calls the more likely one -- made rather than noticed. The reason it is not a
# throwaway instance: the job is one crate plus `mvn test`, which would pay a
# Hetzner boot and a cold cargo cache for a build the warm desktop cache already
# has, and it runs once per landing rather than once per push. (#1672)
EXPECTED_BUILD_JOBS = 8

CHECK = "scripts/ci/cargo_target_health.sh"

# `cargo fmt` and `cargo --version` write nothing to the build directory, so a
# job that only runs those has nothing to protect.
COMPILES = re.compile(r"\bcargo\s+(\+\S+\s+)?(build|test|check|clippy|run|bench|install|doc)\b")


def runs_on_persistent(job: dict) -> bool:
    """True for the desktop, false for an instance made and destroyed per run."""
    runs_on = job.get("runs-on")
    labels = [runs_on] if isinstance(runs_on, str) else list(runs_on or [])
    text = " ".join(str(label) for label in labels)
    if "${{" in text:
        # `runs-on: ${{ needs.create-runner.outputs.label }}` — an ephemeral
        # Hetzner instance. It starts empty every time and is deleted after.
        return False
    return "self-hosted" in text


def step_text(step: dict) -> str:
    return f"{step.get('run', '')}\n{step.get('uses', '')}\n{step.get('name', '')}"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    # Overridable so the cases can build pipelines that do not exist. Defaults
    # are the real thing; nothing in CI passes these.
    ap.add_argument("--workflows", default=str(WORKFLOWS))
    ap.add_argument("--min-workflows", type=int, default=MINIMUM_WORKFLOWS)
    ap.add_argument("--expect-build-jobs", type=int, default=EXPECTED_BUILD_JOBS)
    args = ap.parse_args()

    workflows = pathlib.Path(args.workflows)
    if not workflows.is_dir():
        print(f"SKIPPED (not a pass): {workflows} is missing", file=sys.stderr)
        return 3

    read = 0
    build_jobs: list[str] = []
    unchecked: list[str] = []

    for path in sorted(workflows.glob("*.yml")) + sorted(workflows.glob("*.yaml")):
        try:
            doc = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError as exc:
            print(f"[persistent-build-dir] FATAL: {path.name} does not parse: {exc}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict):
            continue
        read += 1

        for name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict) or not runs_on_persistent(job):
                continue
            steps = [s for s in (job.get("steps") or []) if isinstance(s, dict)]
            first_build = next(
                (i for i, s in enumerate(steps) if COMPILES.search(str(s.get("run", "")))), None
            )
            if first_build is None:
                continue
            label = f"{path.name}:{name}"
            build_jobs.append(label)
            if not any(CHECK in step_text(s) for s in steps[:first_build]):
                unchecked.append(label)

    if read < args.min_workflows:  # FLOOR
        print(
            f"[persistent-build-dir] FATAL: {read} workflow(s) read, floor is "
            f"{args.min_workflows}. The glob stopped matching, and a scan that finds "
            "nothing reports a clean pipeline.",
            file=sys.stderr,
        )
        return 1

    if len(build_jobs) != args.expect_build_jobs:  # FLOOR (two-way)
        direction = "fewer" if len(build_jobs) < args.expect_build_jobs else "more"
        print(
            f"[persistent-build-dir] FATAL: {len(build_jobs)} job(s) compile on the "
            f"persistent runner; this file expects {args.expect_build_jobs}. That is "
            f"{direction} than declared.\n"
            "  Fewer: either the recogniser stopped seeing them -- in which case the\n"
            "  guard is reporting a clean pipeline while measuring nothing -- or lanes\n"
            "  moved off this machine.\n"
            "  More: a lane moved onto the machine that keeps its state between jobs.\n"
            "  Either way, update EXPECTED_BUILD_JOBS and say why in the commit. (#264)\n"
            f"  Found: {', '.join(sorted(build_jobs))}",
            file=sys.stderr,
        )
        return 1

    if unchecked:
        print(
            f"[persistent-build-dir] FATAL: {len(unchecked)} of {len(build_jobs)} job(s) "
            "compile on the persistent runner without checking the build directory first:",
            file=sys.stderr,
        )
        for label in sorted(unchecked):
            print(f"  {label}", file=sys.stderr)
        print(
            f"\nAdd a step running `bash {CHECK}` above the first cargo call. It clears\n"
            "debris an aborted pipeline left behind and fails within seconds if the\n"
            "directory has stopped answering, instead of the job dying forty minutes in\n"
            "on a message that points at a crate -- or not dying at all, and being reaped\n"
            "an hour later as `no_updates_running`. (#264)",
            file=sys.stderr,
        )
        return 1

    print(
        f"[persistent-build-dir] OK: all {len(build_jobs)} job(s) compiling on the "
        f"persistent runner check the build directory first ({read} workflows read)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
