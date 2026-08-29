#!/usr/bin/env python3
"""The wait for a free instance must fit inside the job that does the waiting.

The one-instance rule makes `create-runner` wait rather than buy a second
machine. That wait had a budget of 900 seconds inside a job capped at 600, so
it could never finish: every wait ended in a cancelled job and a red pipeline,
which reads as a broken build rather than a queue doing its job.

Two numbers in two files that must agree, and nothing made them. This is that
something.
"""
from __future__ import annotations

import pathlib
import re
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = WORTEL / "scripts/ci/reuse_an_idle_instance.sh"
WERKSTROOM = WORTEL / ".github/workflows/ci-ephemeral.yml"
# The wait must leave room for provisioning after it succeeds, not just end
# exactly as the axe falls.
MARGE_SECONDEN = 120
# FLOOR: wait >= 1500s -- because one instance at a time means a second push
# must sit out a whole build, and workspace runs 15 to 19 minutes. A wait
# shorter than a build turns the one-instance rule into a coin flip: whichever
# push is second fails, and it fails looking like a broken pipeline.
MINIMUM_WACHT = 1500


def wachtbudget() -> int | None:
    m = re.search(r'while\s*\[\s*"\$\{wacht\}"\s*-lt\s*(\d+)\s*\]', SCRIPT.read_text())
    return int(m.group(1)) if m else None


def jobtimeout() -> int | None:
    tekst = WERKSTROOM.read_text()
    blok = re.search(r"^  create-runner:.*?(?=^  \w|\Z)", tekst, re.S | re.M)
    if blok is None:
        return None
    m = re.search(r"timeout-minutes:\s*(\d+)", blok.group(0))
    return int(m.group(1)) * 60 if m else None


def main() -> int:
    budget, limiet = wachtbudget(), jobtimeout()
    if budget is None:
        print("FAIL: could not find the wait budget; the guard cannot check what it cannot read")
        return 1
    if limiet is None:
        print("FAIL: create-runner has no timeout-minutes, so the wait has no ceiling to fit in")
        return 1
    if budget < MINIMUM_WACHT:
        print(f"FAIL: the wait is {budget}s but a build takes up to 19 minutes. With "
              "one instance at a time, a second push must outlast the first build or "
              f"it fails for no reason; the floor is {MINIMUM_WACHT}s.")
        return 1
    if budget + MARGE_SECONDEN > limiet:
        print(f"FAIL: the wait may run {budget}s inside a job capped at {limiet}s "
              f"(margin {MARGE_SECONDEN}s). The wait would be cancelled before it could "
              "succeed, so a queue looks like a broken build.")
        return 1
    print(f"[wait-fits] OK: wait {budget}s + margin {MARGE_SECONDEN}s fits in {limiet}s.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
