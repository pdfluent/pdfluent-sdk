#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that compiles on the persistent runner must be ready to: the build
directory it inherited, and the toolchain the runner service does not put on PATH.

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

THE SECOND QUESTION, added 06-09-2026 (#343)
--------------------------------------------
`cargo: command not found`, exit 127, seconds into the job. The runner service's
PATH does not carry `~/.cargo/bin`, and three jobs have now learnt that the
expensive way -- visual-regression.yml on every run since 04-09,
java-bindings.yml while it was written, and ci.yml's `workspace` job on its very
first run. Each was fixed by copying a step out of a job that happened to have
it, which is a rule carried by whoever read the right file.

Six jobs still call cargo without it. They are RECORDED rather than refused, in
`KNOWN_PATHLESS`, because closing master over six workflows nobody landing can
fix in the same push is the refusal this repository has already had to withdraw
elsewhere. The register is compared in both directions, so the class is finite: a
new job cannot join it, and a job that is fixed has to be booked.

FLOOR: workflows read >= 10, and build jobs found == EXPECTED_BUILD_JOBS. A scan
that finds nothing sees no unchecked job either, and reports a clean pipeline in
exactly the same words as a scan that found nothing wrong. The second number is
pinned in both directions -- see the comment on it.

Exit codes:
  0  every persistent build job checks its build directory first, and puts cargo
     on PATH or is recorded as not doing so
  1  one does not, a recorded job now does, or the scan lost sight of the pipeline
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
# 8 -> 9 on 06-09-2026: ci.yml's `workspace` job. It is the whole workspace --
# check, clippy and test -- on a push to master, and it exists because the
# landing lane stopped doing that on the pusher's machine (#343). Upward, and
# made rather than noticed: this is a lane moving ONTO the machine that keeps
# its state, which is the direction the comment above calls the more likely one.
# It is affordable there only because that state is warm, which is the same
# reason it must check the build directory before it starts.
EXPECTED_BUILD_JOBS = 9

CHECK = "scripts/ci/cargo_target_health.sh"

# THE SECOND THING EVERY BUILD ON THIS RUNNER NEEDS FIRST, and the one this file
# was extended to cover on 06-09-2026 (#343).
#
# The runner service's PATH does not carry ~/.cargo/bin, so a step that calls
# cargo without putting it there dies with `cargo: command not found`, exit 127,
# in seconds. It has happened three times: visual-regression.yml on every run
# since 04-09, java-bindings.yml while it was being written, and ci.yml's
# `workspace` job on its very first run -- three jobs, one cause, and the same
# remedy copied by hand each time from a job that already had it. A rule that is
# only carried by the jobs whose authors happened to read another job is exactly
# the state the build-directory half of this file was written to end.
#
# Recognised on WHAT THE STEP DOES, not on its name: it must put the cargo bin
# directory on the PATH the following steps see, which on GitHub Actions means
# writing it to $GITHUB_PATH. A step called "Cargo on PATH" that does not is the
# failure this would otherwise wave through.
PATH_MARKERS = ("GITHUB_PATH", ".cargo/bin")

# The jobs that already call cargo without that step, recorded rather than
# refused. A REGISTER THAT MUST NOT GROW, compared in both directions.
#
# Turning this into a refusal today would close master over six workflows nobody
# landing can be asked to fix in the same push -- the shape that closed it four
# times in 24 hours over the never-green guard, and the reason that one is
# advisory now. What the equality does instead is make the class finite: a new
# job cannot join the list, and a job that is fixed has to be booked here, so the
# room won cannot quietly fill again.
#
# Each of these is a real latent exit 127 on the day it next runs. bench.yml and
# crash-guard.yml have only been cancelled since 28-08; gate-ci.yml has not run
# since May; publish-crates.yml:preflight fires on a release tag, which is the
# worst possible moment to learn this.
KNOWN_PATHLESS = {
    "bench.yml:benchmark",
    "crash-guard.yml:crash-guard",
    "enterprise-acceptance.yml:build",
    "gate-ci.yml:gate",
    "publish-crates.yml:preflight",
    "wasm-gate.yml:wasm-gate",
}

# `cargo fmt` and `cargo --version` write nothing to the build directory, so a
# job that only runs those has nothing to protect.
#
# The wrappers count as well, and that is not a convenience: a job that calls
# `bash scripts/ci/run_test.sh` compiles exactly as much as one that spells the
# cargo line out, and reading only the spelled-out form let ci.yml's `workspace`
# job compile the entire workspace on the persistent runner while this guard
# reported nine jobs as eight. A scan that cannot see a build reports a clean
# pipeline in the same words as one that found nothing wrong. (#343)
COMPILES = re.compile(
    r"\bcargo\s+(\+\S+\s+)?(build|test|check|clippy|run|bench|install|doc)\b"
    r"|\bscripts/ci/run_(build|clippy|test)\.sh\b")


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
    ap.add_argument("--known-pathless", default=None,
                    help="comma-separated <workflow>:<job> labels, for the cases; "
                         "the default is the register in this file")
    args = ap.parse_args()
    known_pathless = (KNOWN_PATHLESS if args.known_pathless is None
                      else {s for s in args.known_pathless.split(",") if s})

    workflows = pathlib.Path(args.workflows)
    if not workflows.is_dir():
        print(f"SKIPPED (not a pass): {workflows} is missing", file=sys.stderr)
        return 3

    read = 0
    build_jobs: list[str] = []
    unchecked: list[str] = []
    pathless: list[str] = []

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
            if not any(all(m in str(s.get("run", "")) for m in PATH_MARKERS)
                       for s in steps[:first_build]):
                pathless.append(label)

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

    new_pathless = sorted(set(pathless) - known_pathless)
    fixed = sorted(known_pathless - set(pathless))
    if fixed:
        print(
            f"[persistent-build-dir] FATAL: {len(fixed)} job(s) put cargo on PATH now "
            "and are still recorded as not doing so: "
            + ", ".join(fixed)
            + "\n  Remove them from KNOWN_PATHLESS, so the room won cannot fill again.",
            file=sys.stderr,
        )
        return 1
    if new_pathless:
        print(
            f"[persistent-build-dir] FATAL: {len(new_pathless)} of {len(build_jobs)} job(s) "
            "call cargo on the persistent runner without putting it on PATH first:",
            file=sys.stderr,
        )
        for label in new_pathless:
            print(f"  {label}", file=sys.stderr)
        print(
            "\nAdd a step above the first cargo call that writes \"$HOME/.cargo/bin\" to\n"
            "$GITHUB_PATH and then checks that `cargo --version` runs -- the shape\n"
            "ci.yml's `guards` and `workspace` jobs, java-bindings.yml and\n"
            "visual-regression.yml all carry. The runner service's PATH does not have it,\n"
            "so without that step every cargo step exits 127 within seconds and the log\n"
            "says nothing about the toolchain. (#292, #343)",
            file=sys.stderr,
        )
        return 1

    print(
        f"[persistent-build-dir] OK: all {len(build_jobs)} job(s) compiling on the "
        f"persistent runner check the build directory first, and all but "
        f"{len(known_pathless)} recorded ones put cargo on PATH ({read} workflows read)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
