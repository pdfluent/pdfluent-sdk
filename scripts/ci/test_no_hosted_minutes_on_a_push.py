#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for no_hosted_minutes_on_a_push.py.

A guard that finds nothing on a clean tree cannot tell you whether it still
recognises anything. Breaking its runner table left it passing, because there
was nothing to miss -- so these cases feed it known-bad workflows and check it
says so.

The tag case is the one that matters: `push` with only `tags:` is a release and
may cost, while the same block with `branches:` added is an ordinary push and
may not. That single line was the whole Actions bill (#274).
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

BEWAKER = pathlib.Path(__file__).with_name("no_hosted_minutes_on_a_push.py")

VULLING = """\
name: Filler {n}
on: {{workflow_dispatch: null}}
jobs:
  j:
    runs-on: ubuntu-latest
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


def workflow(trigger: str, runner: str) -> str:
    return (f"name: Proef\non:\n{trigger}\njobs:\n  bouw:\n    runs-on: {runner}\n"
            "    steps:\n      - run: echo hi\n")


GEVALLEN = [
    ("macos on push-to-branch", "  push:\n    branches: [master]\n", "macos-latest", True),
    ("windows on push-to-branch", "  push:\n    branches: [master]\n", "windows-latest", True),
    ("macos on pull_request", "  pull_request:\n    branches: [master]\n", "macos-latest", True),
    ("macos on schedule", "  schedule:\n    - cron: '0 3 * * *'\n", "macos-latest", True),
    # A tag is a release: it may cost.
    ("macos on a tag only", "  push:\n    tags: ['v*']\n", "macos-latest", False),
    # The shape that cost the budget: tags AND branches in one push block. The
    # tags make it look like a release trigger; the branches make it fire on
    # every commit. Reading only for `tags` excuses exactly the wrong file.
    ("macos on tags plus branches",
     "  push:\n    tags: ['v*']\n    branches: [master]\n", "macos-latest", True),
    # Our own Windows box is free. A guard that stopped skipping self-hosted
    # would flag it, and the label says windows.
    ("self-hosted windows on push",
     "  push:\n    branches: [master]\n", "[self-hosted, windows]", False),
    # `tags` with `branches-ignore` still fires on ordinary branches.
    ("macos on tags plus branches-ignore",
     "  push:\n    tags: ['v*']\n    branches-ignore: ['wip/**']\n", "macos-latest", True),
    ("macos on dispatch", "  workflow_dispatch: null\n", "macos-latest", False),
    # Linux is counted, not refused.
    ("ubuntu on push-to-branch", "  push:\n    branches: [master]\n", "ubuntu-latest", False),
    # Our own machines cost nothing.
    ("self-hosted on push", "  push:\n    branches: [master]\n", "[self-hosted, xfa-fast]", False),
]


def main() -> int:
    fouten: list[str] = []
    for naam, trigger, runner, moet_falen in GEVALLEN:
        with tempfile.TemporaryDirectory() as d:
            m = pathlib.Path(d)
            bouw(m, workflow(trigger, runner))
            r = draai(m)
            gefaald = r.returncode != 0
            if gefaald != moet_falen:
                fouten.append(
                    f"{naam}: expected {'a failure' if moet_falen else 'a pass'}, got "
                    f"exit {r.returncode}\n      {r.stdout.strip()}\n      {r.stderr.strip()[:200]}"
                )

    # A matrix with a self-hosted entry AND a hosted macOS one: skipping on the
    # word "self-hosted" would excuse exactly the expensive half.
    gemengd = ("name: Proef\non:\n  push:\n    branches: [master]\njobs:\n  bouw:\n"
               "    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n"
               "        os: [[self-hosted, xfa-fast], macos-latest]\n"
               "    steps:\n      - run: echo hi\n")
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, gemengd)
        if draai(m).returncode == 0:
            fouten.append("macOS beside a self-hosted entry in one matrix is excused")

    # And the matrix form, which is how node-bindings carried it.
    matrix = ("name: Proef\non:\n  push:\n    branches: [master]\njobs:\n  bouw:\n"
              "    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n"
              "        os: [ubuntu-latest, macos-latest]\n    steps:\n      - run: echo hi\n")
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, matrix)
        if draai(m).returncode == 0:
            fouten.append("macOS hidden in a matrix is not seen")

    # No workflows at all: the floor must speak.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        (m / ".github" / "workflows").mkdir(parents=True)
        r = draai(m)
        if r.returncode == 0 or "floor" not in r.stderr:
            fouten.append(f"the floor does not speak on an empty tree: {r.stderr[:160]}")

    if fouten:
        print("[test-hosted-minutes] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"[test-hosted-minutes] OK: {len(GEVALLEN)} trigger/runner combinations, "
          "the matrix form, and the floor.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
