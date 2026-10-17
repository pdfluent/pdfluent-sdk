#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that compiles on a throwaway instance brings its own toolchain. (#495)

WHAT HAPPENED

Wave 34365955374, 09-09-2026: `visual-regression`, `java-bindings` and
`enterprise-acceptance` all died within forty minutes of each other on
`cargo: command not found`, and the fourth job on the same instance -- the
`workspace` job of ci-ephemeral.yml -- compiled the whole workspace. The
difference between them was one step: `workspace` installs a toolchain, the
other three took cargo from the image's PATH and said so in a comment.

What was NOT established is why that particular instance had no cargo on the
PATH an Actions step gets; the machine was reaped with the wave, as every wave
instance is. What was established is enough: the contract those three jobs rely
on -- "the image carries cargo on PATH" -- is a property of a machine nobody
can go back and look at, and the job that did not rely on it was fine.

WHY A GUARD AND NOT JUST THE FIX

Because the fix is three steps in three files, and the thing that broke was an
assumption none of those files could see: "the image carries cargo on PATH" was
written down in three comments and true in none of them that morning. The image
is repaired too -- the seal links cargo into /usr/local/bin, which is on the
PATH of a shell that reads no profile, and the boot probe refuses to publish an
image whose instances cannot answer `cargo --version` over a non-interactive
ssh. This file is what keeps all three from drifting back: a job that compiles
says where its compiler comes from, and the two image-side repairs stay in the
files that carry them.

WHAT COUNTS AS NEEDING THE COMPILER

A `run:` line that invokes cargo, rustc or rustup, and `Swatinem/rust-cache`,
which runs `rustc -vV` before it can key anything -- that is what
enterprise-acceptance failed on, one step before its own cargo line. The
toolchain step has to come BEFORE the first of those, because a cache action
that cannot find rustc has already failed the job by the time the compiler
arrives.

Exit codes:
  0  every wave job that compiles installs a toolchain first, and the image-side
     repairs are where they were put
  1  one of them does not
  2  nothing could be read: no workflow directory, or so few jobs found that the
     scan is not looking at what it thinks it is
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

try:
    import yaml
except ImportError:  # pragma: no cover - the lane installs pyyaml
    print("a_wave_job_that_compiles_carries_its_toolchain: pyyaml is not installed, "
          "so no workflow could be read.", file=sys.stderr)
    sys.exit(2)

ROOT = pathlib.Path(__file__).resolve().parents[2]

# The runner expressions that mean "the instance this wave bought": the label a
# reusable workflow is handed, and the one ci-ephemeral's own jobs read from the
# job that created the machine.
INSTANCE_RUNNER = re.compile(r"inputs\.runner_label|create-runner\.outputs\.label")

# Anything that puts rustc to work, or asks it who it is.
NEEDS_RUST = re.compile(r"(?:^|[;&|(`]|\$\(|\s)(?:cargo|rustc|rustup)(?:\s|$)")

# Actions that run the compiler before any step of ours does. rust-cache runs
# `rustc -vV` to build its cache key, and that is the line wave 34365955374
# failed on in enterprise-acceptance.
ACTIONS_NEEDING_RUST = ("Swatinem/rust-cache",)

TOOLCHAIN_ACTIONS = ("dtolnay/rust-toolchain", "actions-rs/toolchain")

# A scan that finds nothing approves everything.
MIN_JOBS = 3


def steps_of(job: dict) -> list[dict]:
    steps = job.get("steps")
    return [s for s in steps if isinstance(s, dict)] if isinstance(steps, list) else []


def needs_rust(step: dict) -> str:
    uses = str(step.get("uses", ""))
    for action in ACTIONS_NEEDING_RUST:
        if uses.startswith(action):
            return f"uses: {uses}"
    run = step.get("run")
    if isinstance(run, str):
        for line in run.splitlines():
            bare = line.split("#", 1)[0]
            if NEEDS_RUST.search(bare):
                return f"run: {line.strip()[:80]}"
    return ""


def installs_toolchain(step: dict) -> bool:
    uses = str(step.get("uses", ""))
    return any(uses.startswith(action) for action in TOOLCHAIN_ACTIONS)


def wave_jobs(workflows: pathlib.Path):
    """Every (file, job name, job) that runs on an instance a wave bought."""
    for path in sorted(workflows.glob("*.yml")):
        try:
            document = yaml.safe_load(path.read_text(errors="replace"))
        except yaml.YAMLError as bad:
            yield path, "(unreadable)", {"__error__": str(bad)}
            continue
        if not isinstance(document, dict):
            continue
        jobs = document.get("jobs")
        if not isinstance(jobs, dict):
            continue
        for name, job in jobs.items():
            if not isinstance(job, dict):
                continue
            runner = job.get("runs-on")
            if isinstance(runner, str) and INSTANCE_RUNNER.search(runner):
                yield path, str(name), job


def image_repairs(root: pathlib.Path) -> list[str]:
    """The two halves that are not a workflow step, checked where they live.

    Both were added with the workflow steps and either of them can be tidied
    away by somebody who reads the other as the fix. The seal is what puts cargo
    on the PATH of a shell that reads no profile; the probe is what stops an
    image without it from being published.
    """
    complaints = []
    seal = root / "scripts" / "cos" / "gate_instance_seal.sh"
    probe = root / "scripts" / "ci" / "a_gate_image_answers_ssh.py"
    if not seal.is_file():
        complaints.append(f"{seal} is not there, so nothing seals the image")
    else:
        text = seal.read_text(errors="replace")
        if "/usr/local/bin/$gereedschap" not in text and "/usr/local/bin/cargo" not in text:
            complaints.append(
                "scripts/cos/gate_instance_seal.sh no longer puts cargo in /usr/local/bin. "
                "That is the only directory in the PATH of a shell that reads no profile, "
                "which is what an Actions `run:` step gets (#495)")
        if "cargo --version" not in text:
            complaints.append(
                "scripts/cos/gate_instance_seal.sh no longer checks `cargo --version` on a "
                "bare PATH, so an image without a usable compiler is sealed in silence (#495)")
    if not probe.is_file():
        complaints.append(f"{probe} is not there, so no image is probed before it is published")
    elif "cargo --version" not in probe.read_text(errors="replace"):
        complaints.append(
            "scripts/ci/a_gate_image_answers_ssh.py no longer runs `cargo --version` on an "
            "instance of the candidate image, so an image whose jobs cannot compile would be "
            "published as good (#495)")
    return complaints


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=str(ROOT),
                        help="the checkout to read (a fixture tree, in the test)")
    parser.add_argument("--skip-image", action="store_true",
                        help="only judge the workflows, for a fixture that has no image scripts")
    arguments = parser.parse_args()
    root = pathlib.Path(arguments.repo).resolve()
    workflows = root / ".github" / "workflows"
    if not workflows.is_dir():
        print(f"a_wave_job_that_compiles_carries_its_toolchain: no {workflows}; "
              f"nothing was checked.")
        return 2

    findings: list[str] = []
    seen = 0
    for path, name, job in wave_jobs(workflows):
        if "__error__" in job:
            findings.append(f"{path.name}: could not be parsed: {job['__error__']}")
            continue
        seen += 1
        steps = steps_of(job)
        first_need = None
        why = ""
        for index, step in enumerate(steps):
            reason = needs_rust(step)
            if reason:
                first_need, why = index, reason
                break
        if first_need is None:
            continue
        toolchain = next((index for index, step in enumerate(steps) if installs_toolchain(step)), None)
        if toolchain is None:
            findings.append(
                f"{path.name}: job `{name}` runs on the instance and compiles, with no "
                f"toolchain step at all.\n      first step that needs rust: {why}")
        elif toolchain > first_need:
            findings.append(
                f"{path.name}: job `{name}` installs its toolchain at step {toolchain + 1}, "
                f"after step {first_need + 1} has already needed it.\n      that step: {why}")

    if seen < MIN_JOBS:
        print(f"a_wave_job_that_compiles_carries_its_toolchain: only {seen} job(s) found that run "
              f"on a bought instance, and there should be at least {MIN_JOBS}.")
        print("  The scan is not reading the workflows it thinks it is. A scan that finds "
              "nothing approves everything.")
        return 2

    if not arguments.skip_image:
        findings.extend(image_repairs(root))

    if findings:
        print(f"a_wave_job_that_compiles_carries_its_toolchain: {len(findings)} problem(s) "
              f"over {seen} job(s) on bought instances.")
        for finding in findings:
            print(f"  {finding}")
        print("")
        print("A job that compiles installs a toolchain before the first step that needs one:")
        print("  - uses: dtolnay/rust-toolchain@<pinned sha>   # with: toolchain: stable")
        print("and then puts its bin directory on PATH, because /usr/local/bin/cargo on the")
        print("gate image is a plain binary that shadows rustup's shims. The image is repaired")
        print("as well, in the seal and in the boot probe; neither replaces the step.")
        return 1

    print(f"a_wave_job_that_compiles_carries_its_toolchain: OK — {seen} job(s) on a bought "
          f"instance, every one that compiles brings its toolchain.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
