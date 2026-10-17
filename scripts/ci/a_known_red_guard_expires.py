#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A guard that is red on master runs anyway, loudly, until its date (#333, #510).

WHAT THIS IS FOR

Forty-three guards ran in exactly one place: the `guards` job of `ci.yml`, which
is hosted, and hosted Actions have been stopped since the budget went to EUR 1
on 05-09-2026. A guard whose only job cannot run has stopped measuring, and six
of the forty-three had -- unseen, on master, for days.

Moving them into lanes that run means the next landing meets six failures that
belong to nobody's branch. Leaving them where nothing runs them is worse, and
making six unrelated repairs a condition of every landing is not a trade anyone
would take. So they run, they are loud, and they do not block -- until a date
they cannot outlive.

THE THREE DIRECTIONS

  expired     past `expires`, the exception is over: the guard's failure fails
              this one too. A week is long enough to fix six things and short
              enough that nobody plans around it.
  fixed       an entry whose guard now PASSES is a failure of this check. The
              row has to go, or the register stops describing anything -- the
              same rule this repository applies to every other register.
  absent      an entry naming a script that does not exist is a failure. A
              renamed guard must break this rather than quietly leave the list.

WHAT IT DOES NOT DO

It does not decide which guards are red; a person measured them and wrote the
rows. It does not hide output: every guard's own last lines are printed under
its name, whether it is advisory or refusing, because the reason the six went
unseen was that nothing printed them at all.

Usage:
    python3 scripts/ci/a_known_red_guard_expires.py

Exit codes:
    0  every entry is still within its date and still red
    1  one has expired, or is green, or names a script that is gone
"""
from __future__ import annotations

import datetime
import pathlib
import subprocess
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REGISTER = REPO / "scripts/ci/guards_red_on_master.toml"

# How much of a guard's own output to repeat. Enough to show what it says,
# little enough that six of them do not bury the lane summary.
TAIL_LINES = 6


def today() -> datetime.date:
    return datetime.date.today()


def run_guard(script: str, args: list[str]) -> tuple[int, str]:
    done = subprocess.run(
        [sys.executable, str(REPO / "scripts/ci" / script), *args],
        capture_output=True, text=True, cwd=REPO)
    return done.returncode, (done.stdout + done.stderr)


def judge(entries: list[dict], now: datetime.date,
          runner=run_guard) -> tuple[list[str], list[str]]:
    """Findings that fail this check, and the lines to print for a reader."""
    findings: list[str] = []
    report: list[str] = []

    for entry in entries:
        script = entry["script"]
        args = list(entry.get("args", []))
        expires = entry["expires"]
        ticket = entry["ticket"]

        if not (REPO / "scripts/ci" / script).is_file():
            findings.append(
                f"{script}: the register names a script that does not exist. A "
                f"renamed guard leaves this list by being fixed, not by being lost")
            continue

        code, output = runner(script, args)
        left = (expires - now).days

        if code == 0:
            findings.append(
                f"{script}: PASSES now, so its row is stale. Take it out of "
                f"guards_red_on_master.toml -- a register that only grows has "
                f"stopped measuring (#{ticket})")
            report.append(f"  {script}: now green; remove its row")
            continue

        if now > expires:
            findings.append(
                f"{script}: still red and the exception expired on {expires} "
                f"({-left} day(s) ago). #{ticket} says what it is; this lane "
                f"stops waiting for it")
            report.append(f"  {script}: EXPIRED {expires} -- refusing (#{ticket})")
        else:
            report.append(
                f"  {script}: red since {entry['red_since']}, advisory until "
                f"{expires} ({left} day(s) left), #{ticket}")
        report.append(f"      {entry['says']}")
        for line in output.strip().splitlines()[-TAIL_LINES:]:
            report.append(f"      | {line}")

    return findings, report


def main() -> int:
    if not REGISTER.is_file():
        print(f"[known-red] FAIL: {REGISTER} is missing. Without it these guards "
              "are neither excused nor run, which is the state this file ended",
              file=sys.stderr)
        return 1

    entries = tomllib.loads(REGISTER.read_text(encoding="utf-8")).get("guard", [])
    if not entries:
        # An empty register is the goal, not an error: it means the six were
        # fixed and every guard now runs on its own terms.
        print("[known-red] OK: no guard is excused; the register is empty")
        return 0

    findings, report = judge(entries, today())

    print(f"[known-red] {len(entries)} guard(s) red on master, each held to a date:")
    for line in report:
        print(line)

    if findings:
        print("[known-red] FAIL:", file=sys.stderr)
        for line in findings:
            print(f"  - {line}", file=sys.stderr)
        return 1

    print("[known-red] OK: every excused guard is still red and still inside its "
          "date. Advisory here; it refuses on the day above.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
