#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for persistent_builds_check_their_build_dir.py.

The guard is worth almost nothing if it cannot be shown to fail. A workflow
scanner that has stopped recognising jobs prints the same sentence as one that
found a clean pipeline, and that is not a hypothetical in this repository:
thirty-three guards sat below a failing step for thirty runs, each of them
reading as an installed control while executing nothing.

So every case here builds a pipeline the guard should reject, and asserts it
does. The one that earns its place is the ordering case: a check placed after
the first cargo call passes every plausible "is the check present" test and
protects nothing, because the whole point is failing at the top of the job
rather than forty minutes in.

FLOOR: cases run >= 12.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

GUARD = pathlib.Path(__file__).resolve().parent / "persistent_builds_check_their_build_dir.py"
CHECK = "bash scripts/ci/cargo_target_health.sh"

MINIMUM_CASES = 12

failures: list[str] = []
cases = 0


def expect(name: str, condition: bool, detail: str = "") -> None:
    global cases
    cases += 1
    if condition:
        print(f"  ok   {name}")
    else:
        print(f"  FAIL {name} {detail}")
        failures.append(name)


def run(workflows: pathlib.Path, *extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD), "--workflows", str(workflows), *extra],
        capture_output=True,
        text=True,
        check=False,
    )


def pipeline(root: pathlib.Path, name: str, body: str) -> pathlib.Path:
    """One workflow, plus enough filler to clear the workflow floor.

    The filler is not decoration. Without it every case would trip the
    "the glob stopped matching" floor first, and the case would go green for the
    wrong reason -- which is the same defect the floor exists to catch.
    """
    directory = root / name
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "under-test.yml").write_text(body)
    for i in range(12):
        (directory / f"filler-{i}.yml").write_text(
            "name: Filler\non: push\njobs:\n"
            "  nothing:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n"
        )
    return directory


PERSISTENT_UNCHECKED = """name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - uses: actions/checkout@v4
      - name: Compile
        run: cargo build --release
"""

PERSISTENT_CHECKED = f"""name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - uses: actions/checkout@v4
      - name: The shared build directory is there and undamaged
        run: {CHECK}
      - name: Compile
        run: cargo build --release
"""

PERSISTENT_CHECKED_TOO_LATE = f"""name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - uses: actions/checkout@v4
      - name: Compile
        run: cargo build --release
      - name: The shared build directory is there and undamaged
        run: {CHECK}
"""

EPHEMERAL_UNCHECKED = """name: Under test
on: push
jobs:
  builder:
    runs-on: ${{ needs.create-runner.outputs.label }}
    steps:
      - uses: actions/checkout@v4
      - name: Compile
        run: cargo build --release
"""

HOSTED_UNCHECKED = """name: Under test
on: push
jobs:
  builder:
    runs-on: ubuntu-latest
    steps:
      - name: Compile
        run: cargo build --release
"""

PERSISTENT_FMT_ONLY = """name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - name: Formatting
        run: cargo fmt --all --check
"""


def main() -> int:
    if not GUARD.exists():
        print(f"SKIPPED (not a pass): {GUARD} is missing", file=sys.stderr)
        return 3

    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)

        # --- the failure it exists for -----------------------------------
        r = run(pipeline(root, "unchecked", PERSISTENT_UNCHECKED), "--expect-build-jobs", "1")
        expect("a persistent build with no check fails", r.returncode == 1, f"got {r.returncode}")
        expect("and it names the job", "under-test.yml:builder" in r.stderr, r.stderr.strip()[:160])
        expect("and it names the remedy", "cargo_target_health.sh" in r.stderr, r.stderr.strip()[:160])

        r = run(pipeline(root, "checked", PERSISTENT_CHECKED), "--expect-build-jobs", "1")
        expect("a persistent build with the check passes", r.returncode == 0, r.stderr.strip()[:200])

        # --- the case that separates a guard from a decoration ------------
        #
        # The check is present, spelled correctly, and useless: the build has
        # already run. Any test that only asks "does the file mention the
        # script" goes green here.
        r = run(pipeline(root, "toolate", PERSISTENT_CHECKED_TOO_LATE), "--expect-build-jobs", "1")
        expect("a check placed after the build fails", r.returncode == 1, f"got {r.returncode}")

        # --- who is exempt, and why ---------------------------------------
        r = run(pipeline(root, "ephemeral", EPHEMERAL_UNCHECKED), "--expect-build-jobs", "0")
        expect("an ephemeral runner is exempt", r.returncode == 0, r.stderr.strip()[:200])

        r = run(pipeline(root, "hosted", HOSTED_UNCHECKED), "--expect-build-jobs", "0")
        expect("a hosted runner is exempt", r.returncode == 0, r.stderr.strip()[:200])

        r = run(pipeline(root, "fmtonly", PERSISTENT_FMT_ONLY), "--expect-build-jobs", "0")
        expect("cargo fmt is not a build", r.returncode == 0, r.stderr.strip()[:200])

        # --- the two-way ratchet -------------------------------------------
        one = pipeline(root, "ratchet", PERSISTENT_CHECKED)
        r = run(one, "--expect-build-jobs", "2")
        expect("fewer build jobs than declared fails", r.returncode == 1, f"got {r.returncode}")
        expect("and it says which direction", "fewer than declared" in r.stderr, r.stderr.strip()[:200])

        r = run(one, "--expect-build-jobs", "0")
        expect("more build jobs than declared fails", r.returncode == 1, f"got {r.returncode}")
        expect("and it says which direction", "more than declared" in r.stderr, r.stderr.strip()[:200])

        # --- a scan that has stopped scanning ------------------------------
        empty = root / "empty"
        empty.mkdir()
        r = run(empty, "--expect-build-jobs", "0")
        expect("an empty workflow directory is not a pass", r.returncode == 1, f"got {r.returncode}")
        expect(
            "and it says the glob stopped matching",
            "stopped matching" in r.stderr,
            r.stderr.strip()[:200],
        )

        r = run(root / "does-not-exist")
        expect("a missing workflow directory is announced", r.returncode == 3, f"got {r.returncode}")
        expect(
            "and announced as not-a-pass",
            "SKIPPED (not a pass)" in r.stderr,
            r.stderr.strip()[:200],
        )

    if cases < MINIMUM_CASES:  # FLOOR
        print(
            f"\n{cases} case(s) ran, floor is {MINIMUM_CASES}. The script stopped short.",
            file=sys.stderr,
        )
        return 1
    if failures:
        print(f"\n{len(failures)} case(s) failed: {', '.join(failures)}", file=sys.stderr)
        return 1
    print(f"\npersistent_builds_check_their_build_dir: {cases} cases, all as intended")
    return 0


if __name__ == "__main__":
    sys.exit(main())
