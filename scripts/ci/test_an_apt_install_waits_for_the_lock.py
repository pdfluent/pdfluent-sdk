#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The apt-lock guard bites, and does not bite what it should not (#448).

A guard that reports nothing looks exactly like a tree that is clean, so the
cases are fed to it directly: a bare `apt-get` must be found, a wrapped one must
not, and a comment about apt-get must not be mistaken for a call.

The last check runs the guard over the real tree. Both halves are needed: a
guard that finds everything is as useless as one that finds nothing, and only
the pair says which of the two this is.

Exit codes:
  0  the guard reports exactly the calls that do not wait
  1  it misses one, or invents one
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "an_apt_install_waits_for_the_lock.py"

sys.path.insert(0, str(GUARD.parent))
import an_apt_install_waits_for_the_lock as guard  # noqa: E402

BARE = """
jobs:
  build:
    steps:
      - name: Install mutool
        run: sudo apt-get install -y mupdf-tools
"""

WRAPPED = """
jobs:
  build:
    steps:
      - name: Install mutool
        run: bash scripts/ci/apt_get.sh install -y mupdf-tools
"""

COMMENTED = """
jobs:
  build:
    steps:
      # THE STEP THAT FAILED FIFTY-SIX TIMES. It was `sudo apt-get install`
      # with no `update` in front of it.
      - name: Install mutool
        run: bash scripts/ci/apt_get.sh install -y mupdf-tools
"""

APT_SPELLED_OUT = """
jobs:
  build:
    steps:
      - run: sudo apt install -y mupdf-tools
"""


def main() -> int:
    failures: list[str] = []

    found = guard.offending_lines(BARE)
    if len(found) != 1:
        failures.append(f"a bare `sudo apt-get install` must be reported; got {found}")

    if guard.offending_lines(WRAPPED):
        failures.append("a call through the wrapper must not be reported")
    if guard.wrapper_calls(WRAPPED) != 1:
        failures.append("a call through the wrapper must be counted")

    if guard.offending_lines(COMMENTED):
        failures.append(
            "apt-get inside a comment is prose about a past failure, not a call"
        )

    if len(guard.offending_lines(APT_SPELLED_OUT)) != 1:
        failures.append("`apt install` is the same call spelled differently")

    result = subprocess.run(
        [sys.executable, str(GUARD)], capture_output=True, text=True
    )
    if result.returncode != 0:
        failures.append(
            "the guard is red on this tree, so a workflow calls apt without "
            f"waiting:\n{result.stdout}{result.stderr}"
        )

    if failures:
        print("[apt-lock-test] FAIL", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("[apt-lock-test] OK: the guard finds bare apt calls and only those")
    return 0


if __name__ == "__main__":
    sys.exit(main())
