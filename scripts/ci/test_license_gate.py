#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""The four ways `license_gate.py` must fail, on fixtures rather than on us.

Testing a gate by breaking the real lockfile proves it once and leaves the
repository in a state someone has to remember to undo. Everything here runs
against strings and throwaway directories.

The judgements below are the ones that decide whether a copyleft dependency
gets in, so each is pinned rather than left to a reading of the code:

  * a disjunction needs one acceptable option, a conjunction needs all,
  * an unclassified licence is refused and reported as unclassified, which is a
    different instruction to the reader than "forbidden",
  * MPL passes for the editor and fails for the SDK, from the same policy,
  * an unreadable manifest fails; skipping it would turn a broken scanner into
    a green tick.

Exit codes:
    0  every judgement holds
    1  one does not
"""

from __future__ import annotations

# FLOOR: cases >= 16 -- this file is the specification of the gate's judgement,
# and a shortened list is a quietly widened policy.
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("lg", HERE / "license_gate.py")
assert spec and spec.loader
lg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lg)

OK = {"MIT", "Apache-2.0", "BSD-3-Clause", "LicenseRef-PDFluent-Commercial"}
ZWAK = {"MPL-2.0", "LGPL-2.1-or-later"}
VERBODEN = {"GPL-3.0-only", "AGPL-3.0-or-later", "SSPL-1.0"}

FLOOR_CASES = 16

# (expression, weak-copyleft permitted here, expected verdict, what it pins)
CASES: list[tuple[str, set, bool, str]] = [
    ("MIT", set(), True, "a plain allowed licence"),
    ("Apache-2.0", set(), True, "another"),
    ("GPL-3.0-only", set(), False, "a forbidden licence, alone"),
    ("AGPL-3.0-or-later", set(), False, "the one that would end the commercial offer"),
    ("MIT OR Apache-2.0", set(), True, "a disjunction where both are fine"),
    ("MIT OR GPL-3.0-only", set(), True, "a disjunction: one good option is enough"),
    ("GPL-3.0-only OR SSPL-1.0", set(), False, "a disjunction with no good option"),
    ("MIT/Apache-2.0", set(), True, "the older slash spelling of a disjunction"),
    ("Apache-2.0 AND MIT", set(), True, "a conjunction where both are fine"),
    ("MIT AND GPL-3.0-only", set(), False, "a conjunction: one bad part poisons it"),
    ("WTFPL", set(), False, "an unclassified licence is refused"),
    ("", set(), False, "an empty expression is refused, not waved through"),
    # The same expression, two surfaces, one policy.
    ("MPL-2.0", {"MPL-2.0"}, True, "MPL where the surface permits it (editor)"),
    ("MPL-2.0", set(), False, "MPL where it does not (SDK)"),
    ("LGPL-2.1-or-later", set(), False, "LGPL is classified and permitted nowhere"),
    ("MIT OR Apache-2.0 OR LGPL-2.1-or-later", set(), True,
     "JNA's shape: an LGPL option we decline, next to one we take"),
    ("Apache-2.0 WITH LLVM-exception", set(), False,
     "a WITH-exception not in this test's allow set stays unclassified"),
]


def zin(expr: str, zwak_ok: set) -> bool:
    goed, _ = lg.toegestaan(expr, OK, ZWAK, VERBODEN, zwak_ok)
    return goed


def reden(expr: str, zwak_ok: set) -> str:
    return lg.toegestaan(expr, OK, ZWAK, VERBODEN, zwak_ok)[1]


def main() -> int:
    if len(CASES) < FLOOR_CASES:  # FLOOR
        print(f"[test_license_gate] FATAL: {len(CASES)} cases, floor is {FLOOR_CASES}. "
              f"A shortened list is a quietly widened policy.", file=sys.stderr)
        return 1

    fouten = []
    for expr, zwak_ok, verwacht, wat in CASES:
        got = zin(expr, zwak_ok)
        if got != verwacht:
            fouten.append(f"{expr!r} (weak={sorted(zwak_ok)}): expected {verwacht}, got {got} — {wat}")

    # Unclassified must say so, not merely fail: the reader has to know whether
    # to look a licence up or to take it out.
    if "not classified" not in reden("WTFPL", set()):
        fouten.append("an unclassified licence must be reported as unclassified")
    if "forbidden" not in reden("GPL-3.0-only", set()):
        fouten.append("a forbidden licence must be reported as forbidden")

    # An unreadable manifest fails; it is never skipped.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text("{ this is not json")
        echt, lg.REPO = lg.REPO, bed
        try:
            lg.scan_npm({})
            fouten.append("a package.json that will not parse was skipped instead of failing")
        except lg.Onleesbaar:
            pass
        finally:
            lg.REPO = echt

    # A manifest that parses but declares a dependency we have not classified.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text(json.dumps(
            {"name": "x", "license": "MIT", "dependencies": {"left-pad": "^1.0.0"}}))
        # The scanner reads two manifests; a fixture with one makes it raise
        # Onleesbaar and the case below would never run.
        (bed / "crates/xfa-wasm").mkdir(parents=True)
        (bed / "crates/xfa-wasm/Cargo.toml").write_text('[package]\nname = "xfa-wasm"\n')
        echt, lg.REPO = lg.REPO, bed
        try:
            gevonden = lg.scan_npm({"own_packages":
                                     {"crates/xfa-wasm/Cargo.toml": "MIT"}})
        finally:
            lg.REPO = echt
        if not any("left-pad" in w for w, _ in gevonden):
            fouten.append("a declared npm runtime dependency was not reported at all")
        elif all(zin(e, set()) for _, e in gevonden):
            fouten.append("an npm dependency whose licence was never read counted as allowed")

    # A manifest that is simply absent must fail too. This is the case CI found
    # and the local run did not: pkg/package.json is generated and gitignored,
    # so it exists on a developer machine and nowhere else. The scanner used to
    # skip a missing file, which made it see two manifests here and one there.
    with tempfile.TemporaryDirectory() as tmp:
        bed = Path(tmp)
        (bed / "crates/pdf-node").mkdir(parents=True)
        (bed / "crates/pdf-node/package.json").write_text(json.dumps({"license": "MIT"}))
        echt, lg.REPO = lg.REPO, bed
        try:
            lg.scan_npm({})
            fouten.append("a missing second manifest was skipped instead of failing")
        except lg.Onleesbaar:
            pass
        finally:
            lg.REPO = echt

    print(f"[test_license_gate] {len(CASES)} expression case(s) + 3 scanner case(s)")
    if not fouten:
        print("[test_license_gate] every judgement holds")
        return 0
    print(f"[test_license_gate] {len(fouten)} wrong:")
    for f in fouten:
        print(f"  {f}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
