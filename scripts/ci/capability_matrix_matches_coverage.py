#!/usr/bin/env python3
"""A capability the matrix ticks must not be a gap the coverage report names.

docs/wasm-capability-matrix.md marks `renderPageToCanvas` with a tick. The
same repository's api_coverage.py lists it as uncovered, and the only canvas
test checks two helper functions rather than the exported one. So the document
a reader consults says yes while the measurement says no, and both are in
version control (#127).

The tick is not wrong about the feature existing -- it is wrong about it being
verified, and a reader cannot tell those apart from a table.

This does not decide which side is right. It requires that a capability the
matrix ticks either has coverage or is listed here with a reason, so the two
cannot disagree silently.
"""
from __future__ import annotations

import pathlib
import re
import subprocess
import sys

WORTEL = pathlib.Path(__file__).resolve().parents[2]
MATRIX = WORTEL / "docs/wasm-capability-matrix.md"

# Ticked in the matrix, reported as uncovered, with the reason. These two need
# a real browser: the exported function takes a canvas element, and the test
# harness at scripts/ci/qr10_wasm_browser/ exists but no CI file calls it.
ERKEND_ONGEDEKT = {
    "renderPageToCanvas": "#127 -- needs a browser; harness exists, no job runs it",
    "renderPageToCanvasVector": "#127 -- same, vector path",
    # Found by this check on 30-08, and not in #127: the matrix ticks these two
    # for WASM as well, and the coverage report calls both uncovered. Whether
    # the tick or the measurement is wrong is a question for that issue; what
    # matters here is that the disagreement is written down.
    "signatures": "#127 -- ticked for WASM, reported uncovered, no test calls it",
    "text": "#127 -- ticked for WASM, reported uncovered, no test calls it",
}

SCHOON = {k: v for k, v in __import__("os").environ.items() if not k.startswith("GIT_")}


def ongedekt() -> set[str]:
    r = subprocess.run([sys.executable, str(WORTEL / "scripts/ci/api_coverage.py")],
                       capture_output=True, text=True, cwd=WORTEL, env=SCHOON)
    return set(re.findall(r"ongedekt:\s*(\w+)", r.stdout))


def getikt() -> set[str]:
    if not MATRIX.exists():
        return set()
    namen = set()
    for regel in MATRIX.read_text().splitlines():
        if not regel.startswith("|") or "✅" not in regel:
            continue
        for m in re.finditer(r"`(\w+)\(", regel):
            namen.add(m.group(1))
    return namen


def main() -> int:
    if not MATRIX.exists():
        print(f"FAIL: {MATRIX.relative_to(WORTEL)} is missing", file=sys.stderr)
        return 1

    gaten, ticks = ongedekt(), getikt()
    if not ticks:
        print("FAIL: no ticked capabilities parsed from the matrix; the check "
              "cannot have looked.", file=sys.stderr)
        return 1

    botst = (ticks & gaten) - set(ERKEND_ONGEDEKT)
    opgelost = set(ERKEND_ONGEDEKT) - gaten

    if botst:
        print(f"FAIL: {len(botst)} capability(ies) are ticked in the matrix and "
              f"reported as uncovered: {', '.join(sorted(botst))}. A reader "
              "consults the table and cannot see the measurement disagreeing "
              "with it.", file=sys.stderr)
        return 1
    if opgelost:
        print(f"FAIL: {', '.join(sorted(opgelost))} now has coverage and is still "
              "listed as a known gap. Remove it so the next one is noticed.",
              file=sys.stderr)
        return 1

    print(f"[matrix] OK: {len(ticks)} ticked capability(ies), {len(ERKEND_ONGEDEKT)} "
          "recorded gap(s), no silent disagreement.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
