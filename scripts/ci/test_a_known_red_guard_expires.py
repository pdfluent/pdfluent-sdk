#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The expiring exception expires, and lets go of what was fixed (#333, #510).

An exception list is only safe in three directions at once: it must stop
excusing on its date, it must refuse a row whose guard is green again, and it
must refuse a row naming a script that is gone. Each case here drives one of
them with a stub runner, so the test says what the rule is rather than what one
tree happens to contain. The last block runs the real register.
"""
from __future__ import annotations

import datetime
import pathlib
import sys
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import a_known_red_guard_expires as guard  # noqa: E402

MINIMUM_CASES = 8  # FLOOR

DAY = datetime.date(2026, 9, 10)


def entry(script="test_skip_lint.py", expires=datetime.date(2026, 9, 17), **over):
    row = {"script": script, "red_since": DAY, "expires": expires,
           "ticket": 510, "says": "a sentence about what it says"}
    row.update(over)
    return row


def still_red(script, args):
    return 1, "[stub] the guard is still red\n"


def now_green(script, args):
    return 0, "[stub] OK\n"


def main() -> int:
    failures: list[str] = []
    cases = 0

    def case(what: str, ok: bool, detail: str = "") -> None:
        nonlocal cases
        cases += 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:400]}"))
        if not ok:
            failures.append(what)

    found, report = guard.judge([entry()], DAY, still_red)
    case("a red guard inside its date does not fail the lane", found == [], str(found))
    case("and the reader is told when it stops being excused",
         any("advisory until 2026-09-17" in line for line in report), str(report))
    case("and the guard's own words are repeated, not hidden",
         any("still red" in line for line in report), str(report))

    # The day itself is still inside the exception; the day after is not.
    found, _ = guard.judge([entry()], datetime.date(2026, 9, 17), still_red)
    case("the expiry day itself is still excused", found == [], str(found))

    found, _ = guard.judge([entry()], datetime.date(2026, 9, 18), still_red)
    case("the day after the expiry refuses",
         any("expired" in x for x in found), str(found))

    found, _ = guard.judge([entry()], datetime.date(2026, 10, 1), still_red)
    case("and it says how long it has been over",
         any("14 day(s) ago" in x for x in found), str(found))

    # The second direction: fixed, so the row must go.
    found, _ = guard.judge([entry()], DAY, now_green)
    case("a row whose guard is green again is a failure",
         any("stale" in x for x in found), str(found))

    # The third: a renamed guard leaves the list by being fixed, not by being lost.
    found, _ = guard.judge([entry(script="a_guard_that_was_renamed.py")], DAY, still_red)
    case("a row naming a script that does not exist is a failure",
         any("does not exist" in x for x in found), str(found))

    print("\n  -- the real register --")
    rows = tomllib.loads(guard.REGISTER.read_text(encoding="utf-8")).get("guard", [])
    case("every row carries a ticket, a measured date and an expiry after it",
         all(r.get("ticket") and r.get("red_since") and r.get("expires")
             and r["expires"] > r["red_since"] and r.get("says") for r in rows),
         str(rows))
    found, _ = guard.judge(rows, guard.today())
    case("and the register is true today: every row still red, still in date",
         found == [], str(found))

    if cases < MINIMUM_CASES:
        print(f"\nFAIL: {cases} cases ran, at least {MINIMUM_CASES} were written")
        return 1
    if failures:
        print(f"\nFAIL: {len(failures)} of {cases} cases")
        return 1
    print(f"\nOK: {cases} cases")
    return 0


if __name__ == "__main__":
    sys.exit(main())
