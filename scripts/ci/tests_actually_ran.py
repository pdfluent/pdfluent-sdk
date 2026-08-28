#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Run a binding's test suite and refuse to call zero tests a success.

WHAT WENT WRONG

On this machine the .NET suite did not run at all. Thirty-nine tests, every one
of them failing to load, because the native library had been built for the other
processor architecture. `dotnet test` reported no failures, because nothing got
far enough to fail. The job was green.

Zero passed and everything passed produce the same exit code. That is the whole
defect: the number that distinguishes them was printed and nobody read it.

HOW THIS READS THE NUMBER

Each of the five bindings uses a different runner, so there are five formats to
recognise. A runner that changes its wording, or a build that dies before the
summary, leaves nothing to match -- and that case FAILS. It has to: a parser
that shrugs at unrecognised output is how a check stops looking without anyone
noticing, and the alternative reading ("no summary, so probably fine") is
exactly the state this guard exists to detect.

Counts from several summary lines are added, because `make -C ... run` executes
two C binaries and each prints its own.

Usage:
    tests_actually_ran.py <runner> -- <command> [args...]

    runner: capi | dotnet | java | node | python

Exit codes:
    0  the command succeeded and a positive test count was reported
    1  the command failed, reported zero tests, or printed no count at all
"""

# ONDERGRENS: herkende runners >= 5 — capi, dotnet, java, node en python.
# Raakt er een kwijt, dan draait die bindingjob weer kaal en is nul tests
# weer niet te onderscheiden van alle tests. Vastgezet in
# scripts/ci/test_tests_actually_ran.py, dat de vijf sleutels controleert.

from __future__ import annotations

import re
import subprocess
import sys

# Each pattern must capture a group named `n`: the number of tests that ran.
#
# Written against the output these runners produce today, and deliberately not
# made lenient. `Tests run: (\d+)` would also match a sentence about tests, and
# a false count is worse than no count -- it restores the green tick this guard
# is here to remove.
FORMATS: dict[str, list[re.Pattern[str]]] = {
    # Results: 12/14 passed, 2 skipped, 0 failed   (test_smoke)
    # 6/6 tests passed                             (test_text_blocks)
    "capi": [
        re.compile(r"^Results:\s+\d+/(?P<n>\d+)\s+passed", re.M),
        re.compile(r"^(?:\s*)\d+/(?P<n>\d+)\s+tests passed", re.M),
    ],
    # Passed!  - Failed: 0, Passed: 39, Skipped: 0, Total: 39, Duration: ...
    "dotnet": [re.compile(r"Total:\s*(?P<n>\d+)", re.M)],
    # Tests run: 24, Failures: 0, Errors: 0, Skipped: 0
    "java": [re.compile(r"^\[INFO\]\s+Tests run:\s*(?P<n>\d+),", re.M),
             re.compile(r"^Tests run:\s*(?P<n>\d+),", re.M)],
    # Tests:       31 passed, 31 total
    "node": [re.compile(r"^Tests:.*?(?P<n>\d+) total", re.M)],
    # 41 passed, 3 skipped in 12.10s
    "python": [re.compile(r"^(?P<n>\d+) passed", re.M),
               re.compile(r"(?P<n>\d+) passed(?:,|\s+in\b)", re.M)],
}


# A summary that exists and reports nothing executed. Without these, a run
# where every test was skipped matches no count pattern and gets diagnosed as
# "no summary line found" -- which points at this guard when the real cause is
# the binding. Sending the reader to the wrong place is the same defect the
# MissingDependency guard exists to prevent.
ZERO: dict[str, list[re.Pattern[str]]] = {
    "capi": [re.compile(r"^Results:\s+0/0\s+passed", re.M)],
    "dotnet": [re.compile(r"No test (?:is available|matches the given testcasefilter)", re.M)],
    "java": [re.compile(r"^\[INFO\]\s+No tests to run", re.M)],
    "node": [re.compile(r"^Tests:\s+0 total", re.M)],
    # "2 skipped in 0.01s" with no passed count: pytest ran the file and
    # executed nothing. That is how the .NET suite looked, in python spelling.
    "python": [re.compile(r"^\d+ skipped(?:, \d+ \w+)* in ", re.M),
               re.compile(r"^no tests ran in ", re.M)],
}


def total(runner: str, text: str) -> int | None:
    """Sum every summary line this runner produces. None if none matched.

    Matches are de-duplicated by the span they cover, because the two roles a
    second pattern can play are indistinguishable from the pattern list alone.
    For `capi` the patterns are two different lines and both counts belong in
    the total: `make run` executes two C binaries. For `python` they are two
    spellings of one line, and summing them would report 82 tests where 41 ran.
    Overlapping spans mean the same line, so it is counted once; disjoint spans
    mean separate summaries, so they add up.
    """
    taken: list[tuple[int, int]] = []
    found = None
    for pattern in FORMATS[runner]:
        for m in pattern.finditer(text):
            if any(m.start() < end and start < m.end() for start, end in taken):
                continue
            taken.append((m.start(), m.end()))
            found = (found or 0) + int(m.group("n"))
    if found is None and any(z.search(text) for z in ZERO.get(runner, [])):
        return 0
    return found


def main() -> int:
    if len(sys.argv) < 4 or sys.argv[2] != "--":
        print(__doc__.split("Usage:")[1].split("Exit codes:")[0], file=sys.stderr)
        return 1
    runner, command = sys.argv[1], sys.argv[3:]
    if len(FORMATS) < 5:  # ONDERGRENS
        print(f"[tests_actually_ran] FATAL: {len(FORMATS)} runner format(s) known, below "
              f"the declared floor of 5. A binding whose format went missing runs bare "
              f"again, and zero tests stops being distinguishable from all of them.",
              file=sys.stderr)
        return 1
    if runner not in FORMATS:
        print(f"[tests_actually_ran] unknown runner {runner!r}; "
              f"known: {', '.join(sorted(FORMATS))}", file=sys.stderr)
        return 1

    print(f"[tests_actually_ran] {runner}: {' '.join(command)}", flush=True)

    captured: list[str] = []
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, bufsize=1)
    assert proc.stdout is not None
    for line in proc.stdout:
        sys.stdout.write(line)          # keep the job log readable
        captured.append(line)
    code = proc.wait()
    text = "".join(captured)

    ran = total(runner, text)

    print()
    if code != 0:
        print(f"[tests_actually_ran] the suite failed (exit {code}).")
        if ran is not None:
            print(f"[tests_actually_ran] {ran} test(s) ran before that.")
        return 1

    if ran is None:
        print(f"[tests_actually_ran] FAIL: exit 0, but no {runner} summary line was found.")
        print("[tests_actually_ran] That is not the same as 'no tests failed'. Either the")
        print("[tests_actually_ran] suite died before printing its total, or the runner")
        print("[tests_actually_ran] changed its wording and this guard has gone blind.")
        print("[tests_actually_ran] Both need a person; neither is a pass.")
        return 1

    if ran == 0:
        print("[tests_actually_ran] FAIL: exit 0, and zero tests ran.")
        print("[tests_actually_ran] Nothing failed because nothing executed. On this machine")
        print("[tests_actually_ran] the .NET suite did exactly that: 39 tests, all failing to")
        print("[tests_actually_ran] load, because the native library was built for the other")
        print("[tests_actually_ran] architecture. Green means tested, or it means nothing.")
        return 1

    print(f"[tests_actually_ran] ok: {ran} test(s) ran and the suite passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
