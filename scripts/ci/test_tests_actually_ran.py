#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""The count parsers in `tests_actually_ran.py`, against real runner output.

These five formats are the guard. If a parser silently stops matching, the
binding job it protects goes back to being green on zero tests, which is the
failure it was written for -- so the parsers need a test of their own, and the
samples below are copied from actual job logs rather than written from memory.

Two of them are the cases that a first version got wrong:

  * `capi` prints two summaries because `make run` executes two binaries, and
    both counts belong in the total.
  * `python` matches two of its own patterns on one line, and adding those
    would report 82 tests where 41 ran.

Telling those apart from the pattern list alone is not possible, so the parser
de-duplicates on the span each match covers. Both cases are pinned here.

Exit codes:
    0  every parser reads its format correctly
    1  a parser has drifted
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent

spec = importlib.util.spec_from_file_location("tar", HERE / "tests_actually_ran.py")
assert spec and spec.loader
tar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tar)

CASES: list[tuple[str, str, int | None]] = [
    # Two binaries, two summaries, one total.
    ("capi", "====================\n"
             "Results: 12/14 passed, 2 skipped, 0 failed\n"
             "Library version: 1.0.0\n"
             "\n6/6 tests passed\n", 20),
    ("capi", "Results: 0/0 passed, 0 skipped, 0 failed\n", 0),
    ("dotnet", "Passed!  - Failed: 0, Passed: 39, Skipped: 0, Total: 39, Duration: 2 s\n", 39),
    # Surefire with and without the [INFO] prefix; `^` keeps them from both
    # matching the same line, so there is nothing to double.
    ("java", "[INFO] Tests run: 24, Failures: 0, Errors: 0, Skipped: 0\n", 24),
    ("java", "Tests run: 24, Failures: 0, Errors: 0, Skipped: 0\n", 24),
    ("node", "Tests:       31 passed, 31 total\nSnapshots:   0 total\n", 31),
    # Both python patterns match this one line. 41, not 82.
    ("python", "41 passed, 3 skipped in 12.10s\n", 41),
    ("python", "0 passed, 12 skipped in 1.0s\n", 0),
    # A summary that says nothing executed reads as 0, not as unreadable. The
    # distinction matters for the message: 0 points at the binding, None points
    # at this guard, and only one of those is where the reader should look.
    ("python", "no tests ran in 0.01s\n", 0),
    # The .NET defect in python spelling: every test skipped, exit 0, and the
    # word "passed" nowhere in the output.
    ("python", "ss                             [100%]\n2 skipped in 0.00s\n", 0),
    ("capi", "Results: 0/0 passed, 0 skipped, 0 failed\n", 0),
    ("node", "Tests:       0 total\nSnapshots:   0 total\n", 0),
    ("java", "[INFO] No tests to run.\n", 0),
    ("dotnet", "No test is available in C:\\x.dll. Make sure test project has\n", 0),
    # Genuinely unreadable: the runner never got as far as a summary.
    ("dotnet", "MSBUILD : error MSB1003: Specify which project or solution\n", None),
    ("node", "npm ERR! missing script: test\n", None),
    ("java", "[ERROR] Failed to execute goal ... The JAVA_HOME variable is not\n", None),
]


def main() -> int:
    bad = []
    for runner, text, want in CASES:
        got = tar.total(runner, text)
        if got != want:
            bad.append((runner, want, got, text.strip().splitlines()[:1]))

    if not tar.FORMATS.keys() >= {"capi", "dotnet", "java", "node", "python"}:
        print("[test_tests_actually_ran] FATAL: a binding lost its parser", file=sys.stderr)
        return 1

    print(f"[test_tests_actually_ran] {len(CASES)} sample(s) across "
          f"{len(tar.FORMATS)} runners")
    if not bad:
        print("[test_tests_actually_ran] every parser reads its format")
        return 0

    print(f"[test_tests_actually_ran] {len(bad)} parser(s) drifted:")
    for runner, want, got, first in bad:
        print(f"  {runner}: expected {want}, got {got}  <- {first}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
