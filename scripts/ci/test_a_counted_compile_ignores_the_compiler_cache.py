#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The rule bites, and it bites on the shape that actually went wrong.

A guard whose good day and whose broken day look the same is worth nothing, and
this one's good day is silence. So each case below builds a small tree and asks
the real guard about it: a counting gate that clears the cache passes, the same
gate with the clearing removed fails, a gate that only compiles is left alone,
and an exemption that no longer names a counting gate is refused.

The last case is the one that keeps the register honest. An exemption that
outlives its subject is how the next real one goes unnoticed -- the same defect
#426 recorded on the error-anchor list, arriving through a different door.
"""
from __future__ import annotations

import importlib.util
import io
import pathlib
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr

HIER = pathlib.Path(__file__).resolve().parent
GUARD = HIER / "a_counted_compile_ignores_the_compiler_cache.py"

fouten: list[str] = []
gedaan = 0


def verwacht(wat: str, ok: bool, detail: str = "") -> None:
    global gedaan
    gedaan += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {wat}" + (f"   [{detail}]" if not ok and detail else ""))
    if not ok:
        fouten.append(wat)


def oordeel(bestanden: dict[str, str], uitgezonderd: dict[str, str] | None = None):
    """Run the real guard over a fabricated scripts/ci, and report its verdict."""
    with tempfile.TemporaryDirectory() as d:
        wortel = pathlib.Path(d)
        ci = wortel / "scripts" / "ci"
        ci.mkdir(parents=True)
        # The floor is a real rule, so the fixture has to clear it rather than
        # be excused from it: a tree with three files must not pass by accident.
        for n in range(30):
            (ci / f"vulling_{n}.py").write_text("# nothing to see\n")
        for naam, inhoud in bestanden.items():
            (ci / naam).write_text(inhoud)

        spec = importlib.util.spec_from_file_location("gcache", GUARD)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        mod.WORTEL = wortel
        mod.MAP = ci
        if uitgezonderd is not None:
            mod.UITGEZONDERD = uitgezonderd
        else:
            mod.UITGEZONDERD = {}
        uit, err = io.StringIO(), io.StringIO()
        with redirect_stdout(uit), redirect_stderr(err):
            rc = mod.main()
        return rc, uit.getvalue() + err.getvalue()


TELT_EN_WIST = '''
import subprocess, os, re
ERROR_LINE = re.compile(r"src/lib\\\\.rs:(\\\\d+):\\\\d+: error(?:\\\\[([^\\\\]]+)\\\\])?: (.*)")
def build(w):
    env = dict(os.environ)
    env.pop("RUSTC_WRAPPER", None)
    env["CARGO_BUILD_RUSTC_WRAPPER"] = ""
    return subprocess.run(["cargo", "build", "--message-format=short"], env=env)
'''

TELT_EN_WIST_NIET = '''
import subprocess, os, re
ERROR_LINE = re.compile(r"src/lib\\\\.rs:(\\\\d+):\\\\d+: error(?:\\\\[([^\\\\]]+)\\\\])?: (.*)")
def build(w):
    return subprocess.run(["cargo", "build", "--message-format=short"])
'''

# A gate that compiles for a verdict rather than for a number. It must be left
# alone: clearing the cache there would throw away the thing that makes a
# landing lane on this machine affordable, and a cache that broke a build shows
# up as a red build rather than as a smaller number.
BOUWT_ALLEEN = '''
import subprocess
def check():
    return subprocess.run(["cargo", "build"]).returncode == 0
'''

rc, log = oordeel({"telt.py": TELT_EN_WIST})
verwacht("a counting gate that clears both spellings passes", rc == 0, log)

rc, log = oordeel({"telt.py": TELT_EN_WIST_NIET})
verwacht("the same gate without the clearing is refused", rc == 1, log)
verwacht("and the refusal names the file", "telt.py" in log, log)

rc, log = oordeel({"bouwt.py": BOUWT_ALLEEN})
verwacht("a gate that compiles for a verdict is left alone", rc == 0, log)
verwacht("and is not counted as a counting gate", "0 gate(s) count" in log, log)

rc, log = oordeel({"telt.py": TELT_EN_WIST_NIET},
                  uitgezonderd={"telt.py": "a written reason"})
verwacht("an exemption with a reason is honoured", rc == 0, log)

rc, log = oordeel({"bouwt.py": BOUWT_ALLEEN},
                  uitgezonderd={"weg.py": "names something that is not there"})
verwacht("an exemption that no longer names a counting gate is refused", rc == 1, log)
verwacht("and says the entry has outlived its subject", "outlives its subject" in log, log)

# And the real tree, which is the only case that can go red on somebody else's
# commit -- which is the point of having it here rather than only in a fixture.
spec = importlib.util.spec_from_file_location("gcache_echt", GUARD)
echt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(echt)
uit, err = io.StringIO(), io.StringIO()
with redirect_stdout(uit), redirect_stderr(err):
    rc = echt.main()
verwacht("this repository passes its own rule", rc == 0, (uit.getvalue() + err.getvalue())[-400:])

print(f"test_a_counted_compile_ignores_the_compiler_cache: {gedaan} case(s), {len(fouten)} failure(s)")
sys.exit(1 if fouten else 0)
