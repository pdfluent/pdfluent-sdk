#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A performance baseline must name the machine it was measured on.

WHY THIS EXISTS

The SLA targets in BENCHMARKS_SLA.md were measured in April 2026 on a Xeon
E-2176G that was decommissioned in August 2026. The targets stayed. Comparing
against them fails in the flattering direction -- a regression measured on a
faster machine still clears a threshold set on a slower one and reports a pass.

The machine type was written down. `run_benchmarks.sh` named every result
`hetzner-e2176g-<date>.json` and stamped `"hardware": "hetzner-e2176g"` into
it, and `check_benchmark_sla.py` printed that value before comparing. All of it
was a string literal, so a run on any machine produced a file claiming the
Xeon. The label could not go stale because it was never a measurement, and
nothing ever asked whether it matched the machine underneath.

That is the shape this guard rules out. Not "is the number right" -- nobody can
answer that from a repository -- but "does anything compare a number against a
machine it was not measured on, without saying so".

WHAT IT CHECKS

  1. The benchmark tooling names no machine. `run_benchmarks.sh` and
     `check_benchmark_sla.py` may not contain a machine identity as a literal.
     The machine comes from the environment at run time and from the registry;
     a constant in the tooling is the defect this issue is about.

  2. Every machine class CI can reach has a registry entry. Discovered from the
     workflows: `server_type:` values and `[self-hosted, <label>]` labels. Add a
     candidate to a workflow and this fails until somebody writes down what it
     measures.

  3. The SLA document says UNCALIBRATED exactly when nothing is calibrated.
     Both directions: uncalibrated numbers must carry the warning, and the
     warning must go when a calibration lands. A stale "UNCALIBRATED" is its own
     kind of lie.

  4. A calibration expires. `calibrated = true` with a `calibrated_on` older
     than `[meta] calibration_valid_days` counts as absent.

  5. A restored criterion baseline names the machine class in its cache key.
     `target/criterion` is the baseline: restoring it is the comparison. A key
     that says only `runner.os` hands a run somebody else's numbers -- which is
     what `nightly.yml` did, on ephemeral instances that are a different
     machine every night.

  6. The floor below, two-way.
"""

# FLOOR: calibrated machine classes >= 0 -- there is no calibrated class today,
# and that is the honest state: the machine the numbers came from is gone, the
# corpus runner does not exist (#276), and the corpus disk is unreadable. The
# ratchet runs both ways on purpose. The day somebody calibrates a class this
# guard goes red and stays red until the floor is raised in the same commit --
# which is also the commit that must drop UNCALIBRATED from BENCHMARKS_SLA.md
# and record which class the numbers now belong to. A calibration that nobody
# announced is how the last one lasted four months past its hardware.

from __future__ import annotations

import argparse
import datetime as dt
import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parent.parent.parent

REGISTRY_REL = pathlib.Path("benchmarks/BASELINE_HARDWARE.toml")
SLA_DOC_REL = pathlib.Path("BENCHMARKS_SLA.md")
WORKFLOWS_REL = pathlib.Path(".github/workflows")

# The tooling that must not name a machine. These two files write and read the
# result JSON; a machine identity in either of them is a constant pretending to
# be provenance.
TOOLING_REL = [
    pathlib.Path("scripts/run_benchmarks.sh"),
    pathlib.Path("scripts/check_benchmark_sla.py"),
]

# Shapes a machine identity takes in this repository. Deliberately concrete: a
# broad "looks like a hostname" rule would match half the documentation.
MACHINE_LITERAL = re.compile(
    r"\b(hetzner[-_a-z0-9]*|e[-_]?2176g|ex42|cx\d{2}|cpx\d{2}|ccx\d{2})\b",
    re.IGNORECASE,
)

# FLOOR: calibrated machine classes >= 0 (see module docstring).
CALIBRATED_FLOOR = 0

# Liveness, not quality: a glob that finds nothing also finds no violations,
# and reads exactly like a clean tree.
MINIMUM_WORKFLOWS = 10

UNCALIBRATED_MARKER = "UNCALIBRATED"

# Criterion keeps the previous run here and compares against it. Restoring this
# directory from a cache is the comparison, so the key is what decides whose
# numbers you are measured against.
CRITERION_PATH = "target/criterion"
CLASS_IN_KEY = "BENCH_MACHINE_CLASS"


def fatal(message: str) -> int:
    print(f"[baseline-hardware] FATAL: {message}", file=sys.stderr)
    return 1


def load_registry(path: pathlib.Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def calibrated_classes(registry: dict, today: dt.date) -> tuple[list[str], list[str]]:
    """Return (calibrated, expired) class ids."""
    meta = registry.get("meta", {})
    valid_days = int(meta.get("calibration_valid_days", 0))
    live: list[str] = []
    expired: list[str] = []

    for name, entry in registry.get("classes", {}).items():
        if not entry.get("calibrated", False):
            continue
        stamp = str(entry.get("calibrated_on", "")).strip()
        if not stamp:
            expired.append(f"{name} (calibrated = true with no calibrated_on)")
            continue
        try:
            when = dt.date.fromisoformat(stamp)
        except ValueError:
            expired.append(f"{name} (calibrated_on {stamp!r} is not a date)")
            continue
        if valid_days and (today - when).days > valid_days:
            expired.append(f"{name} (calibrated {(today - when).days} days ago, "
                           f"limit is {valid_days})")
            continue
        live.append(name)

    return sorted(live), sorted(expired)


def reachable_classes_in_ci(workflows: pathlib.Path) -> tuple[set[str], set[str], int]:
    """(cloud server types, self-hosted labels, workflows read) CI can ask for."""
    types: set[str] = set()
    labels: set[str] = set()
    read = 0

    paths = sorted(workflows.glob("*.yml")) + sorted(workflows.glob("*.yaml"))
    for path in paths:
        text = path.read_text(errors="replace")
        read += 1
        for m in re.finditer(r"^\s*server_type:\s*['\"]?([A-Za-z0-9.-]+)", text, re.M):
            value = m.group(1)
            # `${{ ... }}` and friends are not a type we can look up.
            if not value.startswith("$"):
                types.add(value)
        for m in re.finditer(r"self-hosted\s*,\s*([A-Za-z0-9_.-]+)", text):
            labels.add(m.group(1))

    return types, labels, read


def criterion_cache_keys(workflows: pathlib.Path) -> list[tuple[str, int, str]]:
    """Cache keys that restore a criterion baseline without naming the machine.

    Returns (workflow name, line number, the offending line).
    """
    offenders: list[tuple[str, int, str]] = []

    paths = sorted(workflows.glob("*.yml")) + sorted(workflows.glob("*.yaml"))
    for path in paths:
        lines = path.read_text(errors="replace").splitlines()
        for index, line in enumerate(lines):
            if CRITERION_PATH not in line:
                continue
            # Walk out to the enclosing step: back to its `- ` and on to the
            # next one at the same indent. Cheaper than a YAML dependency, and
            # this guard already has to run wherever python does.
            start = index
            while start > 0 and not lines[start].lstrip().startswith("- "):
                start -= 1
            indent = len(lines[start]) - len(lines[start].lstrip())
            end = start + 1
            while end < len(lines):
                stripped = lines[end].lstrip()
                if stripped.startswith("- ") and (len(lines[end]) - len(stripped)) <= indent:
                    break
                end += 1

            in_keys = False
            for offset in range(start, end):
                text = lines[offset]
                stripped = text.strip()
                is_key = stripped.startswith("key:")
                if stripped.startswith("restore-keys:"):
                    in_keys = True
                    # `restore-keys: |` is a block-scalar header, not a key.
                    remainder = stripped[len("restore-keys:"):].strip().lstrip("|>-").strip()
                    if remainder and CLASS_IN_KEY not in remainder:
                        offenders.append((path.name, offset + 1, stripped))
                    continue
                if is_key:
                    in_keys = False
                    if CLASS_IN_KEY not in stripped:
                        offenders.append((path.name, offset + 1, stripped))
                    continue
                if in_keys:
                    if not stripped or stripped.startswith("-") or ":" in stripped.split("-")[0]:
                        if not stripped or ":" in stripped:
                            in_keys = False
                            continue
                    if CLASS_IN_KEY not in stripped:
                        offenders.append((path.name, offset + 1, stripped))

    return offenders


def main(root: pathlib.Path = REPO, minimum_workflows: int = MINIMUM_WORKFLOWS) -> int:
    REGISTRY = root / REGISTRY_REL
    SLA_DOC = root / SLA_DOC_REL
    WORKFLOWS = root / WORKFLOWS_REL
    TOOLING = [root / rel for rel in TOOLING_REL]

    def rel(path: pathlib.Path) -> pathlib.Path:
        return path.relative_to(root)

    if not REGISTRY.is_file():
        return fatal(f"{rel(REGISTRY)} is missing -- every benchmark "
                     "result has to name a class that is written down somewhere")
    if not SLA_DOC.is_file():
        return fatal(f"{rel(SLA_DOC)} is missing")
    if not WORKFLOWS.is_dir():
        return fatal(f"{rel(WORKFLOWS)} is missing")

    registry = load_registry(REGISTRY)
    classes = registry.get("classes", {})
    if not classes:
        return fatal(f"{rel(REGISTRY)} declares no classes")

    problems: list[str] = []
    today = dt.date.today()

    # --- 1. the tooling names no machine ------------------------------------
    scanned = 0
    for path in TOOLING:
        if not path.is_file():
            return fatal(f"{rel(path)} is missing -- this guard is "
                         "reading nothing and would report a clean tree")
        scanned += 1
        for number, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
            hit = MACHINE_LITERAL.search(line)
            if hit:
                problems.append(
                    f"{rel(path)}:{number} names a machine as a literal "
                    f"({hit.group(0)!r}). The machine comes from BENCH_MACHINE_CLASS "
                    f"at run time; a constant here is provenance that cannot go stale "
                    f"because it was never measured.\n      {line.strip()[:100]}"
                )
    if scanned == 0:
        return fatal("no benchmark tooling scanned")

    # --- 2. every class CI can reach is registered ---------------------------
    types, labels, workflows_read = reachable_classes_in_ci(WORKFLOWS)
    if workflows_read < minimum_workflows:
        return fatal(f"{workflows_read} workflow(s) read, expected at least "
                     f"{minimum_workflows} -- the glob is not finding the workflows, "
                     "so it is not finding unregistered machines either")

    registered_types = {str(e["provisioned_as"]) for e in classes.values()
                        if e.get("provisioned_as")}
    registered_labels = {str(e["runner_label"]) for e in classes.values()
                         if e.get("runner_label")}

    for missing in sorted(types - registered_types):
        problems.append(
            f"workflows provision server_type {missing!r}, which has no entry in "
            f"{rel(REGISTRY)}. Add one with `provisioned_as = "
            f"\"{missing}\"` and write down what it measures, or nothing will ever "
            f"notice a baseline that moved onto it."
        )
    for missing in sorted(labels - registered_labels):
        problems.append(
            f"workflows request self-hosted label {missing!r}, which has no entry in "
            f"{rel(REGISTRY)}. Add one with `runner_label = "
            f"\"{missing}\"`."
        )

    # --- 3 + 4. calibration state, and what the SLA document says about it ---
    live, expired = calibrated_classes(registry, today)
    for note in expired:
        problems.append(
            f"{rel(REGISTRY)}: {note}. An expired calibration counts as "
            f"absent -- re-measure and update calibrated_on, or set calibrated = false."
        )

    sla_text = SLA_DOC.read_text(errors="replace")
    says_uncalibrated = UNCALIBRATED_MARKER in sla_text

    if not live and not says_uncalibrated:
        problems.append(
            f"no machine class is calibrated, but {rel(SLA_DOC)} never "
            f"says {UNCALIBRATED_MARKER}. A reader takes those milliseconds for "
            f"measurements. Say where they came from and that nothing reproduces them."
        )
    if live and says_uncalibrated:
        problems.append(
            f"{', '.join(live)} is calibrated, but {rel(SLA_DOC)} still "
            f"says {UNCALIBRATED_MARKER}. Remove the warning and name the class the "
            f"numbers belong to."
        )

    # --- 5. a restored baseline names the machine it came from ---------------
    for workflow, number, line in criterion_cache_keys(WORKFLOWS):
        problems.append(
            f".github/workflows/{workflow}:{number} restores a criterion baseline "
            f"under a key that does not name the machine class. Restoring "
            f"{CRITERION_PATH} is the comparison: this run gets measured against "
            f"whichever machine last wrote that key. Put ${{{{ env.{CLASS_IN_KEY} }}}} "
            f"in the key and every restore-key.\n      {line}"
        )

    # --- 6. the floor, both ways --------------------------------------------
    if len(live) < CALIBRATED_FLOOR:
        problems.append(
            f"{len(live)} calibrated machine class(es), floor is {CALIBRATED_FLOOR}. "
            f"A calibration was removed without lowering the floor."
        )
    if len(live) > CALIBRATED_FLOOR:
        problems.append(
            f"{len(live)} calibrated machine class(es) ({', '.join(live)}), floor is "
            f"{CALIBRATED_FLOOR}. This is the good direction and it still fails, on "
            f"purpose: raise CALIBRATED_FLOOR to {len(live)} in this file, in the same "
            f"commit that lands the calibration. An improvement nobody wrote down is "
            f"how the last baseline outlived its hardware by four months."
        )

    # --- report -------------------------------------------------------------
    print(f"[baseline-hardware] {len(classes)} machine class(es) registered, "
          f"{len(live)} calibrated (floor {CALIBRATED_FLOOR})")
    print(f"[baseline-hardware] {workflows_read} workflow(s) read, "
          f"{scanned} benchmark script(s) scanned")

    if not problems:
        print("[baseline-hardware] every baseline names a machine that is written down")
        return 0

    print(f"[baseline-hardware] {len(problems)} problem(s):", file=sys.stderr)
    for problem in problems:
        print(f"  - {problem}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo", default=str(REPO),
                    help="tree to inspect; the test drives fixture trees through this")
    ap.add_argument("--minimum-workflows", type=int, default=MINIMUM_WORKFLOWS,
                    help="liveness floor on the workflow glob")
    args = ap.parse_args()
    sys.exit(main(pathlib.Path(args.repo).resolve(), args.minimum_workflows))
