#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The guard against the shape that stopped every wave on 09-09-2026.

Run against a throwaway tree, because the point is what it says about a
workflow pair it has not seen, and the repository's own pair is (by then)
correct. Both halves are checked: the scope that is missing has to be found,
and the four calls that lean on the read-only default token must not be.
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tempfile

HIER = pathlib.Path(__file__).resolve().parent
WACHTER = HIER / "a_caller_grants_what_it_calls.py"

CALLEE_MET_WRITE = """\
name: callee
on:
  workflow_call:
permissions:
  contents: read
  checks: write
jobs:
  doe:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"""

CALLEE_ALLEEN_READ = """\
name: callee
on:
  workflow_call:
permissions:
  contents: read
jobs:
  doe:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"""


def caller(permissions: str) -> str:
    return f"""\
name: caller
on:
  workflow_dispatch:
jobs:
  roept:
{permissions}    uses: ./.github/workflows/callee.yml
"""


GEVALLEN = [
    ("the scope is granted", CALLEE_MET_WRITE,
     caller("    permissions:\n      contents: read\n      checks: write\n"), 0),
    ("the scope is missing", CALLEE_MET_WRITE,
     caller("    permissions:\n      contents: read\n"), 1),
    ("no block at all, and a write is asked for", CALLEE_MET_WRITE,
     caller(""), 1),
    # The false positive that would make this guard unusable: four calls in
    # ci-ephemeral.yml name no permissions and need none, because the default
    # token is read-only and read-only is all they ask for.
    ("no block, and only a read is asked for", CALLEE_ALLEEN_READ,
     caller(""), 0),
    # A job-level request counts too: GitHub compares against everything the
    # called workflow can use, not only its header.
    ("the write is on a job of the callee", CALLEE_ALLEEN_READ.replace(
        "  doe:\n    runs-on: ubuntu-latest",
        "  doe:\n    permissions:\n      checks: write\n    runs-on: ubuntu-latest"),
     caller(""), 1),
]


def main() -> int:
    mislukt = 0
    for naam, callee, roeper, verwacht in GEVALLEN:
        with tempfile.TemporaryDirectory() as tmp:
            boom = pathlib.Path(tmp)
            wf = boom / ".github" / "workflows"
            wf.mkdir(parents=True)
            (wf / "callee.yml").write_text(callee, encoding="utf-8")
            (wf / "caller.yml").write_text(roeper, encoding="utf-8")
            scripts = boom / "scripts" / "ci"
            scripts.mkdir(parents=True)
            kopie = scripts / WACHTER.name
            shutil.copy(WACHTER, kopie)
            uit = subprocess.run([sys.executable, str(kopie)],
                                 capture_output=True, text=True)
            if uit.returncode != verwacht:
                mislukt += 1
                print(f"  WRONG: {naam}: expected exit {verwacht}, got "
                      f"{uit.returncode}\n{uit.stdout}{uit.stderr}",
                      file=sys.stderr)

    # And it must still pass on this repository, or it is not wired to anything.
    eigen = subprocess.run([sys.executable, str(WACHTER)],
                           capture_output=True, text=True)
    if eigen.returncode != 0:
        mislukt += 1
        print("  WRONG: the guard does not pass on this repository:\n"
              f"{eigen.stdout}{eigen.stderr}", file=sys.stderr)

    if mislukt:
        print(f"[caller-grants-test] FAIL: {mislukt} case(s)", file=sys.stderr)
        return 1
    print(f"[caller-grants-test] OK: {len(GEVALLEN)} case(s) plus this repository")
    return 0


if __name__ == "__main__":
    sys.exit(main())
