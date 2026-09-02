#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The baseline-hardware guard has to reject what it claims to reject.

A guard is only worth its runtime if it goes red on the tree it was written
against. The one it was written against is the tree this repository had before
#283: a machine name baked into the benchmark scripts as a literal, and an SLA
document full of milliseconds from a decommissioned machine with nothing saying
so.

So this test builds that tree from scratch and runs the guard over it. Each
case names the defect and asserts a non-zero exit. The final case is the
repaired tree, which must come back clean -- a guard that fails on everything
is as useless as one that fails on nothing, and it is the pair that shows the
verdict tracks the input.

The guard's own two-way floor is exercised here too: a calibrated class is fed
in and the guard must go red, because an improvement nobody wrote down is how
the last baseline outlived its hardware by four months.

Nothing here touches the real tree. Every case runs in a temporary directory.

Exit codes:
    0  the guard behaves
    1  the guard let something through, or rejected something it should not
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

GUARD = pathlib.Path(__file__).resolve().parent / "a_baseline_names_the_machine.py"

CLEAN_REGISTRY = """
[meta]
calibration_valid_days = 180

[classes.retired-class]
description = "The machine the numbers came from"
cores = 12
reachable = false
calibrated = false
calibrated_on = ""

[classes.cloud-class]
description = "Ephemeral cloud instance"
provisioned_as = "cx53"
reachable = true
calibrated = false
calibrated_on = ""

[classes.desktop-class]
description = "Persistent self-hosted runner"
runner_label = "xfa-fast"
reachable = true
calibrated = false
calibrated_on = ""
"""

CLEAN_SLA = """# Performance SLA

> ## UNCALIBRATED
> The numbers below came from a machine that no longer exists.

| Operation | Target |
|---|---|
| Render A4 text-only | < 50 ms |
"""

CLEAN_RUNNER = """#!/usr/bin/env bash
# Output: benchmarks/results/<class>-<date>.json
OUTPUT_JSON="$RESULTS_DIR/${BENCH_MACHINE_CLASS}-${TODAY}.json"
echo '{"machine_class": "'"${BENCH_MACHINE_CLASS}"'"}'
"""

CLEAN_CHECKER = '''#!/usr/bin/env python3
"""Compare a result against SLA targets; refuse when the class is uncalibrated."""
'''

# Enough workflow files to clear the liveness floor the test passes in, one of
# which provisions the cloud class and one of which asks for the desktop label.
WORKFLOW_WITH_TYPE = """
jobs:
  create:
    steps:
      - uses: someone/runner@v1
        with:
          server_type: cx53
"""
WORKFLOW_WITH_LABEL = """
jobs:
  guard:
    runs-on: [self-hosted, xfa-fast]
"""
WORKFLOW_PLAIN = """
jobs:
  build:
    runs-on: ubuntu-latest
"""

# Restoring `target/criterion` is the comparison, so the key decides whose
# numbers this run is measured against. Correct here; a case below strips the
# class back out.
WORKFLOW_WITH_CRITERION = """
jobs:
  bench:
    runs-on: [self-hosted, xfa-fast]
    env:
      BENCH_MACHINE_CLASS: desktop-class
    steps:
      - uses: actions/cache@v4
        with:
          path: target/criterion
          key: bench-${{ env.BENCH_MACHINE_CLASS }}-master-${{ github.sha }}
          restore-keys: |
            bench-${{ env.BENCH_MACHINE_CLASS }}-master-
      - name: Run
        run: cargo bench
"""


def build(root: pathlib.Path) -> None:
    """Write the repaired tree: everything the guard wants, and nothing it hates."""
    (root / "benchmarks").mkdir(parents=True, exist_ok=True)
    (root / "scripts").mkdir(parents=True, exist_ok=True)
    flows = root / ".github" / "workflows"
    flows.mkdir(parents=True, exist_ok=True)

    (root / "benchmarks" / "BASELINE_HARDWARE.toml").write_text(CLEAN_REGISTRY)
    (root / "BENCHMARKS_SLA.md").write_text(CLEAN_SLA)
    (root / "scripts" / "run_benchmarks.sh").write_text(CLEAN_RUNNER)
    (root / "scripts" / "check_benchmark_sla.py").write_text(CLEAN_CHECKER)

    (flows / "provision.yml").write_text(WORKFLOW_WITH_TYPE)
    (flows / "guard.yml").write_text(WORKFLOW_WITH_LABEL)
    (flows / "bench.yml").write_text(WORKFLOW_WITH_CRITERION)
    for n in range(2):
        (flows / f"plain{n}.yml").write_text(WORKFLOW_PLAIN)


def run(root: pathlib.Path) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, str(GUARD), "--repo", str(root), "--minimum-workflows", "5"],
        capture_output=True, text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


# Each case mutates the repaired tree in one way and must be rejected. The
# description is what a reader would have to believe for the mutation to be
# harmless -- which is exactly what was believed until August 2026.
def literal_in_the_runner(root: pathlib.Path) -> None:
    path = root / "scripts" / "run_benchmarks.sh"
    path.write_text(path.read_text().replace(
        '"$RESULTS_DIR/${BENCH_MACHINE_CLASS}-${TODAY}.json"',
        '"$RESULTS_DIR/hetzner-e2176g-${TODAY}.json"'))


def literal_in_the_checker(root: pathlib.Path) -> None:
    path = root / "scripts" / "check_benchmark_sla.py"
    path.write_text(path.read_text() + '\nEXAMPLE = "results/hetzner-e2176g-2026-04-18.json"\n')


def unregistered_server_type(root: pathlib.Path) -> None:
    path = root / ".github" / "workflows" / "provision.yml"
    path.write_text(path.read_text().replace("cx53", "ccx63"))


def unregistered_runner_label(root: pathlib.Path) -> None:
    path = root / ".github" / "workflows" / "guard.yml"
    path.write_text(path.read_text().replace("xfa-fast", "xfa-corpus"))


def sla_stops_saying_uncalibrated(root: pathlib.Path) -> None:
    path = root / "BENCHMARKS_SLA.md"
    path.write_text(path.read_text().replace("UNCALIBRATED", "Baseline"))


def calibration_without_announcing_it(root: pathlib.Path) -> None:
    """The good direction. It must still fail: the floor is a two-way ratchet."""
    path = root / "benchmarks" / "BASELINE_HARDWARE.toml"
    path.write_text(path.read_text().replace(
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = false\ncalibrated_on = ""',
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = true\n'
        'calibrated_on = "2026-08-30"'))


def calibration_that_expired(root: pathlib.Path) -> None:
    path = root / "benchmarks" / "BASELINE_HARDWARE.toml"
    path.write_text(path.read_text().replace(
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = false\ncalibrated_on = ""',
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = true\n'
        'calibrated_on = "2020-01-01"'))
    # And drop the warning, so the only thing left to object to is the age.
    sla = root / "BENCHMARKS_SLA.md"
    sla.write_text(sla.read_text().replace("UNCALIBRATED", "Baseline"))


def calibration_dated_in_the_future(root: pathlib.Path) -> None:
    """A date nobody could have measured on. It must be named as such: without
    the check the entry counts as live, and the guard still goes red -- on the
    floor -- so this case also asserts the reason (codex, #1622)."""
    path = root / "benchmarks" / "BASELINE_HARDWARE.toml"
    path.write_text(path.read_text().replace(
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = false\ncalibrated_on = ""',
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = true\n'
        'calibrated_on = "2999-01-01"'))
    sla = root / "BENCHMARKS_SLA.md"
    sla.write_text(sla.read_text().replace("UNCALIBRATED", "Baseline"))


def calibration_with_no_date(root: pathlib.Path) -> None:
    path = root / "benchmarks" / "BASELINE_HARDWARE.toml"
    path.write_text(path.read_text().replace(
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = false\ncalibrated_on = ""',
        'runner_label = "xfa-fast"\nreachable = true\ncalibrated = true'))
    sla = root / "BENCHMARKS_SLA.md"
    sla.write_text(sla.read_text().replace("UNCALIBRATED", "Baseline"))


def criterion_key_forgets_the_machine(root: pathlib.Path) -> None:
    """The shape `nightly.yml` had: a baseline restored across machines."""
    path = root / ".github" / "workflows" / "bench.yml"
    path.write_text(path.read_text().replace(
        "${{ env.BENCH_MACHINE_CLASS }}", "${{ runner.os }}"))


def criterion_restore_key_forgets_the_machine(root: pathlib.Path) -> None:
    """Only the fallback loses the class -- which is the one that gets used."""
    path = root / ".github" / "workflows" / "bench.yml"
    path.write_text(path.read_text().replace(
        "            bench-${{ env.BENCH_MACHINE_CLASS }}-master-",
        "            bench-master-"))


def registry_removed(root: pathlib.Path) -> None:
    (root / "benchmarks" / "BASELINE_HARDWARE.toml").unlink()


CASES = [
    ("the runner names a machine as a literal again", literal_in_the_runner),
    ("the checker carries a machine name in an example", literal_in_the_checker),
    ("a workflow provisions a type nobody described", unregistered_server_type),
    ("a workflow asks for a label nobody described", unregistered_runner_label),
    ("the SLA drops UNCALIBRATED while nothing is calibrated", sla_stops_saying_uncalibrated),
    ("a class is calibrated without raising the floor", calibration_without_announcing_it),
    ("a calibration is older than the registry allows", calibration_that_expired),
    ("a calibration claims no date", calibration_with_no_date),
    ("a calibration is dated in the future", calibration_dated_in_the_future,
     "is in the future"),
    ("a criterion baseline is restored under a machine-blind key",
     criterion_key_forgets_the_machine),
    ("only the criterion fallback key loses the machine",
     criterion_restore_key_forgets_the_machine),
    ("the registry is gone", registry_removed),
]


def main() -> int:
    failures: list[str] = []

    for description, mutate, *reason in CASES:
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            build(root)
            mutate(root)
            code, output = run(root)
            # A case may name the reason it expects. Red for the wrong reason is
            # not caught: the future-dated calibration trips the two-way floor
            # whether or not the date is ever looked at.
            named = reason[0] if reason else None
            caught = code != 0 and (named is None or named in output)
            verdict = "caught" if caught else "MISSED"
            print(f"  {verdict:7} exit={code}  {description}")
            if not caught:
                failures.append(description)
                if code == 0:
                    print("          the guard reported a clean tree here:")
                else:
                    print(f"          the guard went red, but never said {named!r}:")
                for line in output.strip().splitlines():
                    print(f"          {line}")

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        build(root)
        code, output = run(root)
        print(f"  {'clean' if code == 0 else 'FALSE+':7} exit={code}  the repaired tree")
        if code != 0:
            failures.append("the repaired tree is rejected")
            for line in output.strip().splitlines():
                print(f"          {line}")

    if failures:
        print(file=sys.stderr)
        print(f"[test-baseline-hardware] {len(failures)} case(s) wrong:", file=sys.stderr)
        for description in failures:
            print(f"  - {description}", file=sys.stderr)
        print("A guard that does not bite is a green tick over an untested claim.",
              file=sys.stderr)
        return 1

    print(f"[test-baseline-hardware] {len(CASES)} rejections and one clean tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
