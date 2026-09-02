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

The last two cases run the whole script with a converter that copies, a
veraPDF that says yes, and a mutool that is either absent or refuses one file.
Absent must be the announced skip, exit 3. Refusing a file must be exit 1 with
that file named: the first version returned 3 for both, and the workflow reads
3 as "nothing to calibrate against", so an unreadable output shipped green
(codex, #1617).

Exit codes:
    0  the gate accepts what it should and rejects what it should
    1  the gate has stopped biting
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from pdfa_output_conformance_gate import (  # noqa: E402
    FLOORS,
    MINIMUM_FIXTURES,
    Onleesbaar,
    judge,
    retentie,
)

GATE = Path(__file__).resolve().with_name("pdfa_output_conformance_gate.py")

FAKE_CONVERTER = "#!/bin/sh\ncp \"$1\" \"$2\"\n"
FAKE_VERAPDF = (
    "#!/bin/sh\n"
    "echo '{\"report\":{\"jobs\":[{\"validationResult\":[{\"compliant\":true}]}]}}'\n"
)
# `mutool draw -F txt <file>`: $4 is the file. Reads every source, refuses
# every output the converter wrote.
FAKE_MUTOOL_REFUSES_OUTPUT = (
    "#!/bin/sh\n"
    "case \"$4\" in *.pdfa.pdf) echo unreadable >&2; exit 1;; esac\n"
    "echo some text\n"
)


def schrijf(dir_: Path, naam: str, inhoud: str) -> Path:
    pad = dir_ / naam
    pad.write_text(inhoud)
    pad.chmod(0o755)
    return pad


def draai_gate(mutool: str) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory() as d:
        tmp = Path(d)
        converter = schrijf(tmp, "converter", FAKE_CONVERTER)
        verapdf = schrijf(tmp, "verapdf", FAKE_VERAPDF)
        if mutool == "refuses-output":
            mutool = str(schrijf(tmp, "mutool", FAKE_MUTOOL_REFUSES_OUTPUT))
        return subprocess.run(
            [sys.executable, str(GATE), "--converter", str(converter),
             "--verapdf", str(verapdf), "--mutool", mutool, "--out", str(tmp / "out")],
            capture_output=True, text=True, timeout=300,
        )


def unreadable_cases() -> list[bool]:
    """mutool absent is a skip; mutool refusing a file is a failure naming it."""
    goed: list[bool] = []

    # The pure half, cheap: a reader that cannot read the output names it.
    def lezer(p: Path):
        return None if p.name.endswith(".pdfa.pdf") else b"abc"
    try:
        retentie(lezer, Path("a.pdf"), Path("a.pdfa.pdf"), {})
        print("  FAIL  retentie() swallowed an unreadable output")
        goed.append(False)
    except Onleesbaar as e:
        ok = e.path.name == "a.pdfa.pdf"
        print(f"  {'ok  ' if ok else 'FAIL'}  retentie() names the side it could not read: {e.path.name}")
        goed.append(ok)

    if os.name == "nt":
        print("SKIPPED (not a pass): the end-to-end cases use sh scripts as fake tools, "
              "which do not run on Windows.", file=sys.stderr)
        return goed

    r = draai_gate("/nonexistent/mutool-for-this-test")
    ok = r.returncode == 3 and "SKIPPED (not a pass): mutool" in r.stderr
    print(f"  {'ok  ' if ok else 'FAIL'}  no mutool installed: exit {r.returncode}, "
          f"expected 3 with the skip announced")
    if not ok:
        print(r.stdout + r.stderr)
    goed.append(ok)

    r = draai_gate("refuses-output")
    ok = (r.returncode == 1 and "FATAL" in r.stderr
          and "could not read" in r.stderr and ".pdfa.pdf" in r.stderr)
    print(f"  {'ok  ' if ok else 'FAIL'}  mutool refuses an output: exit {r.returncode}, "
          f"expected 1 naming the file")
    if not ok:
        print(r.stdout + r.stderr)
    goed.append(ok)
    return goed

FLOOR = {"fixtures": 5, "converted": 5, "conformant": 5, "retained": 5}
FLOORS_UNDER_TEST = {"linux": dict(FLOOR)}


def case(naam: str, measured: dict, key: str, verwacht: int,
         floors: dict | None = None) -> bool:
    gekregen, regels = judge(measured, FLOORS_UNDER_TEST if floors is None else floors, key)
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
        # measurement of a different document. Refusing beats guessing -- but
        # what "refusing" means depends on whether we claim to gate there.
        #
        # A platform we do NOT gate on has nothing to be measured against, so
        # failing every run on evidence we do not have would make this a
        # permanent red. It measures, prints the numbers to record, and says it
        # did not judge: exit 3. (codex, #1617)
        case(
            "an ungated platform with no floor announces, and does not judge",
            dict(FLOOR),
            "sunos",
            3,
        ),
        # A platform we DO claim to gate on, with no floor, is a contradiction
        # in the file itself and must be loud: one of the two is wrong.
        case(
            "a gated platform with no floor is an error in this file",
            dict(FLOOR),
            "darwin",
            1,
            floors={},
        ),
        # And it does pass when everything matches, or it is not a gate but a
        # tripwire.
        case("everything matches the floor", dict(FLOOR), "linux", 0),
    ]

    print("[test-pdfa-output] unreadable is a failure, absent is a skip:")
    goed += unreadable_cases()

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
