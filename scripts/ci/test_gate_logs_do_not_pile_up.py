#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The local gate throws its log directories away, and only the right ones.

`local_ci_gate.sh` makes a directory per run, because two lanes running at once
used to overwrite each other's evidence and there was no signal when they did.
It removes that directory when the run is clean and keeps it when the run is
red -- the failing run is the one whose logs somebody wants.

What nobody removed was the kept ones. They stood in $TMPDIR for weeks, beside
the two-gigabyte build directories of the site-blocks gate that took this
machine from 72 GB free to 35 GB in three hours on 05-09-2026 (#344). These are
kilobytes, so this is not a disk floor; it is that a directory nobody removes
is a directory nobody removes.

So a clean lane also sweeps the `lcg.` directories older than a day. That is
the half worth a test, because both of its mistakes are silent. Sweeping too
little leaves the pile. Sweeping too much takes the logs another terminal is
reading right now -- three of them share this machine -- and the reader finds
an empty path where the failure was, which is exactly the confusion the
per-run directory was introduced to end.

The function is lifted out of the gate by name and run on a $TMPDIR of our
own. Running the gate itself would take an hour and touch the real one.

FLOOR: cases >= 6 -- this file builds its own, and a run that executes two of
them has stopped short rather than found a clean tree.
"""

import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
GATE = REPO / "scripts" / "ci" / "local_ci_gate.sh"
FLOOR = 6
TWO_DAYS = 2 * 24 * 3600


def the_function() -> str:
    """`sweep_logs`, as the gate defines it, or nothing if it is gone."""
    text = GATE.read_text(encoding="utf-8")
    m = re.search(r"^sweep_logs\(\) \{\n(.*?)^\}\n", text, re.S | re.M)
    return f"sweep_logs() {{\n{m.group(1)}}}\n" if m else ""


def sweep(function: str, tmp: Path, logdir: Path, fail: int) -> None:
    """Run the gate's clean-up as the gate runs it, on a $TMPDIR of our own."""
    script = (
        "set -uo pipefail\n"
        f"{function}"
        f'LOGDIR={logdir}\n'
        f"fail={fail}\n"
        "sweep_logs\n"
    )
    subprocess.run(["bash", "-c", script], env={**os.environ, "TMPDIR": str(tmp)},
                   capture_output=True, text=True, check=False)


def age(path: Path, seconds: float) -> None:
    old = time.time() - seconds
    os.utime(path, (old, old))


def make(tmp: Path, name: str, seconds: float = 0) -> Path:
    d = tmp / name
    d.mkdir()
    (d / "clippy.log").write_text("...\n")
    if seconds:
        age(d, seconds)
    return d


def cases(function: str) -> list[tuple[str, bool, str]]:
    out = []

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        mine = make(tmp, "lcg.AAAAAA")
        stale = make(tmp, "lcg.BBBBBB", TWO_DAYS)
        live = make(tmp, "lcg.CCCCCC")
        foreign = make(tmp, "siteblocks-test", TWO_DAYS)
        sweep(function, tmp, mine, fail=0)

        out.append((
            "a clean lane throws away its own logs",
            not mine.exists(),
            f"still there={mine.exists()}",
        ))
        out.append((
            "a clean lane throws away the logs kept by an older red lane",
            not stale.exists(),
            f"still there={stale.exists()}",
        ))
        out.append((
            "a lane another terminal is still writing to is left alone",
            live.exists(),
            f"still there={live.exists()}",
        ))
        out.append((
            "it takes its own kind and nothing else in $TMPDIR",
            foreign.exists(),
            f"still there={foreign.exists()}",
        ))

    with tempfile.TemporaryDirectory() as raw:
        tmp = Path(raw)
        mine = make(tmp, "lcg.DDDDDD")
        stale = make(tmp, "lcg.EEEEEE", TWO_DAYS)
        sweep(function, tmp, mine, fail=1)

        out.append((
            "a red lane keeps its own logs -- they are the evidence",
            mine.exists(),
            f"still there={mine.exists()}",
        ))
        out.append((
            "a red lane sweeps nothing at all, not even the old ones",
            stale.exists(),
            f"still there={stale.exists()}",
        ))

    return out


def main() -> int:
    function = the_function()
    if not function:
        print(
            "[gate-logs] local_ci_gate.sh no longer defines `sweep_logs`.\n"
            "  Either the clean-up is gone, in which case the log directories "
            "pile up again (#344),\n  or it was renamed and this file is "
            "checking a function nobody calls.",
            file=sys.stderr,
        )
        return 1
    if "trap sweep_logs EXIT" not in GATE.read_text(encoding="utf-8"):
        print(
            "[gate-logs] `sweep_logs` is defined but no longer hangs on EXIT, "
            "so nothing runs it.",
            file=sys.stderr,
        )
        return 1

    results = cases(function)
    if len(results) < FLOOR:
        print(
            f"[gate-logs] only {len(results)} cases. This file builds them "
            "itself, so that is not a clean tree.",
            file=sys.stderr,
        )
        return 1

    for name, ok, _ in results:
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")

    failed = [(n, got) for n, ok, got in results if not ok]
    if failed:
        print("\n[gate-logs] the gate's log clean-up is not right:\n", file=sys.stderr)
        for name, got in failed:
            print(f"  - {name}\n      got: {got}", file=sys.stderr)
        return 1

    print(f"[gate-logs] {len(results)} cases, all good")
    return 0


if __name__ == "__main__":
    sys.exit(main())
