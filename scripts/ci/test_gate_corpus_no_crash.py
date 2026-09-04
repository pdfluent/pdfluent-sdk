#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The no-crash gate stops at its budget while running, not after.

The gate's budget existed and was compared once the pool had drained. A corpus
where every file hangs would therefore be reported about an hour in -- 500
files, 30 s each, four at a time -- which is after the job itself has been
killed, and a killed job reports nothing (codex, #1627). This drives the gate
with a binary that never returns and a budget of one second, and requires the
verdict within seconds.

The control is a binary that returns at once with a generous budget: the
deadline must not turn a healthy run red.

Nothing here reads the real corpus or the real binary. The corpus is five
hundred empty files -- the floor the gate insists on -- and the binary is a
shell script.

Exit codes:
    0  the gate stops at its budget and passes a fast run
    1  it does not
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys
import tempfile
import time

GATE = pathlib.Path(__file__).resolve().with_name("gate_corpus_no_crash.py")
VLOER = 500

SLOW = "#!/bin/sh\nsleep 30\n"
FAST = "#!/bin/sh\nexit 0\n"

# The gate must answer well inside this; the old gate would still be on its
# first four files.
PATIENCE_S = 40


def corpus(root: pathlib.Path) -> pathlib.Path:
    d = root / "corpus"
    d.mkdir()
    for i in range(VLOER):
        (d / f"f{i:03}.pdf").write_bytes(b"")
    return d


def binary(root: pathlib.Path, body: str) -> pathlib.Path:
    b = root / "renderer"
    b.write_text(body)
    b.chmod(0o755)
    return b


def run(root: pathlib.Path, body: str, budget: float) -> tuple[int | None, str, float]:
    cmd = [sys.executable, str(GATE), "--binary", str(binary(root, body)),
           "--corpus", str(corpus(root)), "--known", str(root / "none.json"),
           "--workers", "4", "--budget", str(budget), "--timeout", "30"]
    t0 = time.monotonic()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=PATIENCE_S)
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b"").decode(errors="replace") + (e.stderr or b"").decode(errors="replace")
        return None, out, time.monotonic() - t0
    return r.returncode, r.stdout + r.stderr, time.monotonic() - t0


def main() -> int:
    if os.name == "nt":
        print("SKIPPED (not a pass): the fake renderer is a shell script, which does "
              "not run on Windows.", file=sys.stderr)
        return 0

    fouten: list[str] = []

    with tempfile.TemporaryDirectory() as d:
        rc, out, took = run(pathlib.Path(d), SLOW, budget=1)
        if rc is None:
            fouten.append(f"the gate did not stop: still running {PATIENCE_S} s after a "
                          f"1 s budget with a renderer that never returns")
        elif rc != 1 or "budget" not in out:
            fouten.append(f"a renderer that never returns, budget 1 s: rc={rc}, "
                          f"expected 1 with a budget verdict\n{out}")
        elif took > 15:
            fouten.append(f"the budget verdict took {took:.0f} s to arrive on a 1 s budget")
        else:
            print(f"  ok    a run that cannot finish is stopped at its budget ({took:.1f} s)")

    with tempfile.TemporaryDirectory() as d:
        rc, out, took = run(pathlib.Path(d), FAST, budget=900)
        if rc != 0 or "OK" not in out:
            fouten.append(f"a renderer that returns at once, budget 900 s: rc={rc}, "
                          f"expected 0\n{out}")
        else:
            print(f"  ok    a fast run is not touched by the deadline ({took:.1f} s)")

    if fouten:
        print("[test-no-crash] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-no-crash] OK: the budget is enforced while running")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
