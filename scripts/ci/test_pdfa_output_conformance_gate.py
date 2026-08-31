#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The PDF/A output gate must have a failing branch, and this proves it has one.

This exists because the gate it replaces did not. `verapdf.yml` published

    | Total | 5 |  | Passed | 0 |  | Pass Rate | 0% |

on pull request #1543 and the job around it was green, and nobody could tell
from the workflow file alone -- the failing branch was in
`scripts/run-verapdf.sh` behind a `--ci` flag the workflow never passed.

So the first case below is that exact report. A conformance result of 0% must
come back red, and it must keep coming back red when somebody rewrites the
comparison later.

Costs nothing to run: it drives `judge()` with numbers, so it needs no veraPDF,
no mutool, no cargo and no corpus. That is why it can sit in the cheap guard job
that runs on every pull request, next to the expensive gate it vouches for.

Exit codes:
    0  the gate accepts what it should and rejects what it should
    1  the gate has stopped biting
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pdfa_output_conformance_gate import (  # noqa: E402
    FLOORS,
    MINIMUM_FIXTURES,
    judge,
)

FLOOR = {"fixtures": 5, "converted": 5, "conformant": 5, "retained": 5}
FLOORS_UNDER_TEST = {"linux": dict(FLOOR)}


def case(naam: str, measured: dict, key: str, verwacht: int) -> bool:
    gekregen, regels = judge(measured, FLOORS_UNDER_TEST, key)
    ok = gekregen == verwacht
    print(f"  {'ok  ' if ok else 'FAIL'}  {naam}: exit {gekregen}, expected {verwacht}")
    if not ok:
        for regel in regels:
            print(f"          {regel}")
    return ok


def main() -> int:
    print("[test-pdfa-output] the gate's failing branch:")
    goed = [
        # The report verapdf.yml actually posted, and shipped green.
        case(
            "the 0% report that shipped green on #1543",
            {"fixtures": 5, "converted": 5, "conformant": 0, "retained": 5},
            "linux",
            1,
        ),
        case(
            "one document stopped conforming",
            {"fixtures": 5, "converted": 5, "conformant": 4, "retained": 5},
            "linux",
            1,
        ),
        # Conformance intact, text gone: the failure mode a pass rate cannot see.
        case(
            "conformant but a document lost its text",
            {"fixtures": 5, "converted": 5, "conformant": 5, "retained": 4},
            "linux",
            1,
        ),
        case(
            "a conversion crashed",
            {"fixtures": 5, "converted": 4, "conformant": 4, "retained": 4},
            "linux",
            1,
        ),
        # The upward half of the ratchet. Nothing is wrong with the number; the
        # point is that it moved and no human edited the floor to say so.
        case(
            "a number rose without an edit to the floor",
            {"fixtures": 6, "converted": 6, "conformant": 6, "retained": 6},
            "linux",
            1,
        ),
        # An empty corpus/ glob: converts nothing, grades nothing, and without
        # this branch reports it in the same words as a clean run.
        case(
            "the fixture set went missing",
            {"fixtures": 0, "converted": 0, "conformant": 0, "retained": 0},
            "linux",
            1,
        ),
        # Font substitution is OS-dependent, so another platform's floor is a
        # measurement of a different document. Refusing beats guessing.
        case(
            "no floor recorded for this platform",
            dict(FLOOR),
            "sunos",
            1,
        ),
        # And it does pass when everything matches, or it is not a gate but a
        # tripwire.
        case("everything matches the floor", dict(FLOOR), "linux", 0),
    ]

    if MINIMUM_FIXTURES < 5:
        print(
            f"  FAIL  MINIMUM_FIXTURES is {MINIMUM_FIXTURES}; the corpus holds five "
            "conversion inputs and a floor below that stops catching an empty glob"
        )
        goed.append(False)

    # A floor that grades nothing passes everything. This is the shape of the
    # bug, not a style rule: the deleted workflow's empty-corpus branch wrote
    # `"total":0` and exited 0.
    for key, floor in FLOORS.items():
        if floor.get("conformant", 0) < 1:
            print(f"  FAIL  FLOORS[{key!r}] demands no conformant output at all")
            goed.append(False)

    if all(goed):
        print("[test-pdfa-output] the gate still bites")
        return 0
    print("[test-pdfa-output] FATAL: the gate has stopped biting", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
