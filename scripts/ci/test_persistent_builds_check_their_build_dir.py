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
    """The guard over a fabricated pipeline, with an EMPTY pathless register.

    The register in the guard names jobs in the real workflows, and a fabricated
    pipeline contains none of them -- which the guard would correctly read as
    "six recorded jobs put cargo on PATH now". Every case that is about something
    else therefore starts from an empty register, and the cases that ARE about it
    pass their own.
    """
    if not any(a == "--known-pathless" for a in extra):
        extra = ("--known-pathless", "", *extra)
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

# What a job on the persistent runner has to do before its first cargo call:
# clear the build directory it inherited, and put cargo on a PATH the runner
# service does not carry.
ONPATH = 'echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"'

PERSISTENT_CHECKED = f"""name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - uses: actions/checkout@v4
      - name: The shared build directory is there and undamaged
        run: {CHECK}
      - name: Cargo on PATH
        run: {ONPATH}
      - name: Compile
        run: cargo build --release
"""

# Checked, but the toolchain is never put on PATH. This is what ci.yml's
# `workspace` job was on its first run: three cargo steps, exit 127 each,
# seconds apart, on a runner whose service PATH has no ~/.cargo/bin. (#343)
PERSISTENT_NO_PATH = f"""name: Under test
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

# The PATH step after the build, which helps nothing: the step that needed it
# has already exited 127.
PERSISTENT_PATH_TOO_LATE = f"""name: Under test
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
      - name: Cargo on PATH
        run: {ONPATH}
"""

# A step CALLED "Cargo on PATH" that puts nothing on it. The name is what a
# reader checks and it is exactly what must not be enough.
PERSISTENT_PATH_IN_NAME_ONLY = f"""name: Under test
on: push
jobs:
  builder:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - uses: actions/checkout@v4
      - name: The shared build directory is there and undamaged
        run: {CHECK}
      - name: Cargo on PATH
        run: echo "cargo is on the path, honest"
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
      - name: Cargo on PATH
        run: {ONPATH}
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

        # --- the second thing a build on this runner needs first (#343) ---
        #
        # The runner service's PATH does not carry ~/.cargo/bin, so a job that
        # calls cargo without putting it there exits 127 in seconds and says
        # nothing about the toolchain. It has now happened to three jobs, the
        # last of them the workspace job this guard's own ratchet was raised for.
        r = run(pipeline(root, "nopath", PERSISTENT_NO_PATH), "--expect-build-jobs", "1")
        expect("a persistent build that never puts cargo on PATH fails",
               r.returncode == 1, f"got {r.returncode}")
        expect("and it names that job too",
               "under-test.yml:builder" in r.stderr, r.stderr.strip()[:200])
        expect("and it names the remedy, not just the symptom",
               "GITHUB_PATH" in r.stderr, r.stderr.strip()[:200])
        # Same shape as the build-directory case above, and the same reason: a
        # step that runs after the build helps the step that already failed.
        r = run(pipeline(root, "pathlate", PERSISTENT_PATH_TOO_LATE), "--expect-build-jobs", "1")
        expect("a PATH step placed after the build fails", r.returncode == 1, f"got {r.returncode}")
        # WHAT THE STEP DOES, not what it is called. A step named "Cargo on
        # PATH" that puts nothing on it is the exact thing a name check waves
        # through, and it is the likeliest way for this to rot.
        r = run(pipeline(root, "pathname", PERSISTENT_PATH_IN_NAME_ONLY),
                "--expect-build-jobs", "1")
        expect("a step named for PATH that changes none fails",
               r.returncode == 1, f"got {r.returncode}")

        # --- the register, in both directions -----------------------------
        #
        # Six jobs already call cargo without the step. Refusing them today would
        # close master over workflows nobody landing can fix in the same push, so
        # they are recorded -- and a record that only looks one way lets the room
        # it wins fill up again.
        recorded = run(pipeline(root, "nopath", PERSISTENT_NO_PATH),
                       "--expect-build-jobs", "1",
                       "--known-pathless", "under-test.yml:builder")
        expect("a recorded job does not refuse the run",
               recorded.returncode == 0, recorded.stderr.strip()[:200])
        expect("and the summary says how many are recorded",
               "1 recorded" in recorded.stdout, recorded.stdout.strip()[:200])
        stale = run(pipeline(root, "checked", PERSISTENT_CHECKED),
                    "--expect-build-jobs", "1",
                    "--known-pathless", "under-test.yml:builder")
        expect("a job that is fixed and still recorded fails",
               stale.returncode == 1, f"got {stale.returncode}")
        expect("and says the entry has to go",
               "still recorded" in stale.stderr, stale.stderr.strip()[:200])

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
