#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for every_workflow_can_start.py.

The guard passes on this repository, which on its own says nothing: it passed
on 30-08-2026 too, when it did not exist and three workflows had been dead for
days. So the cases below hand it the exact file that was broken -- `env:` with
the value deleted from under it -- along with the same shape at job and step
level, and check that it says so.

The pass cases matter as much. `on: workflow_dispatch:` is a null value and is
correct; a guard that refuses every null would refuse most of this repository,
be turned off within the day, and take the real check with it.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

BEWAKER = pathlib.Path(__file__).with_name("every_workflow_can_start.py")

VULLING = """\
name: Filler {n}
on:
  workflow_dispatch:
jobs:
  j:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - run: echo hi
"""


def bouw(map_: pathlib.Path, proef: str) -> None:
    wf = map_ / ".github" / "workflows"
    wf.mkdir(parents=True, exist_ok=True)
    for n in range(12):
        (wf / f"filler{n}.yml").write_text(VULLING.format(n=n))
    (wf / "proef.yml").write_text(proef)


def draai(map_: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(BEWAKER)], cwd=map_,
                          capture_output=True, text=True, check=False)


# (name, workflow, must_fail)
GEVALLEN = [
    # Exactly avrt.yml / docs-drift-guard.yml / wasm-surface-guard.yml as they
    # stood on 31-08-2026: the entry deleted from env:, the key left behind.
    ("the 31-08 file: top-level env: with nothing under it",
     "name: Proef\non:\n  workflow_dispatch:\n\nenv:\n\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n", True),
    ("job-level env: with nothing under it",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    env:\n    steps:\n      - run: echo hi\n", True),
    ("step-level env: with nothing under it",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n        env:\n",
     True),
    ("a `with:` block emptied of its inputs",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n"
     "      - uses: actions/checkout@v4\n        with:\n", True),
    ("runs-on: with no runner",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n    runs-on:\n"
     "    steps:\n      - run: echo hi\n", True),
    ("steps: with no steps",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n", True),
    ("jobs: with no jobs",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n", True),
    ("on: with no trigger",
     "name: Proef\non:\njobs:\n  j:\n    runs-on: [self-hosted, xfa-fast]\n"
     "    steps:\n      - run: echo hi\n", True),
    ("a job declared and left empty",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n", True),

    # --- and the shapes that are correct and must stay correct ---

    # The null that is right: the event name is the whole statement. This is how
    # nearly every workflow here is triggered.
    ("on: workflow_dispatch: -- a null that is correct",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n", False),
    ("on: push: with no filters -- also correct",
     "name: Proef\non:\n  push:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n", False),
    # No env: at all is how you say "no environment". The fix, not the fault.
    ("no env: key at all",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n", False),
    ("an env: that has something in it",
     "name: Proef\non:\n  workflow_dispatch:\nenv:\n  CARGO_TERM_COLOR: always\n"
     "jobs:\n  j:\n    runs-on: [self-hosted, xfa-fast]\n    steps:\n      - run: echo hi\n",
     False),
    # `on:` quoted is the string key, not the boolean True. Both must be read.
    ("a quoted \"on\" key, empty",
     "name: Proef\n\"on\":\njobs:\n  j:\n    runs-on: [self-hosted, xfa-fast]\n"
     "    steps:\n      - run: echo hi\n", True),
    # A matrix axis may legitimately be called env. It is not a workflow `env:`.
    ("a matrix axis named env is not the env: key",
     "name: Proef\non:\n  workflow_dispatch:\njobs:\n  j:\n"
     "    runs-on: [self-hosted, xfa-fast]\n    strategy:\n      matrix:\n"
     "        env: [dev, prod]\n    steps:\n      - run: echo ${{ matrix.env }}\n", False),
]


def main() -> int:
    fouten: list[str] = []
    for naam, proef, moet_falen in GEVALLEN:
        with tempfile.TemporaryDirectory() as d:
            m = pathlib.Path(d)
            bouw(m, proef)
            r = draai(m)
            gefaald = r.returncode != 0
            if gefaald != moet_falen:
                fouten.append(
                    f"{naam}: expected {'a failure' if moet_falen else 'a pass'}, got "
                    f"exit {r.returncode}\n      {r.stdout.strip()}\n"
                    f"      {r.stderr.strip()[:240]}"
                )

    # The floor. A glob that matches nothing must not report a clean tree --
    # that is indistinguishable from a repository where nothing can start.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        (m / ".github" / "workflows").mkdir(parents=True)
        r = draai(m)
        if r.returncode == 0 or "floor" not in r.stderr:
            fouten.append(f"the floor does not speak on an empty tree: {r.stderr[:160]}")

    # A missing .github/workflows is not a pass either.
    with tempfile.TemporaryDirectory() as d:
        if draai(pathlib.Path(d)).returncode == 0:
            fouten.append("a tree with no .github/workflows at all reports OK")

    if fouten:
        print("[test-startbaar] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"[test-startbaar] OK: {len(GEVALLEN)} workflow shapes, the floor, and a "
          "tree with no workflows directory.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
