#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for workflow_inputs_are_not_shell.py.

The guard's whole value is that it keeps finding things. These tests are about
the ways it could stop: a context it no longer recognises, a `run:` block shape
it no longer parses, and a workflow directory it can no longer see.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile

GUARD = pathlib.Path(__file__).with_name("workflow_inputs_are_not_shell.py")

SCHOON = """\
name: Clean
on:
  workflow_dispatch:
    inputs:
      crate: {default: pdfluent}
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - env:
          INPUT_CRATE: ${{ inputs.crate }}
        run: cargo package -p "$INPUT_CRATE"
"""


def draai(map_: pathlib.Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(GUARD)],
        cwd=map_,
        capture_output=True,
        text=True,
        check=False,
    )


def bouw(map_: pathlib.Path, bestanden: dict[str, str]) -> None:
    wf = map_ / ".github" / "workflows"
    wf.mkdir(parents=True, exist_ok=True)
    for naam, inhoud in bestanden.items():
        (wf / naam).write_text(inhoud)


def vullen(aantal: int) -> dict[str, str]:
    """Enough clean workflows and run-blocks to clear both floors."""
    uit = {}
    for n in range(aantal):
        stappen = "\n".join(f'      - run: echo step {i}' for i in range(8))
        uit[f"filler{n}.yml"] = (
            f"name: Filler {n}\non: {{push: {{branches: [master]}}}}\n"
            f"jobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n{stappen}\n"
        )
    return uit


GEVALLEN: list[tuple[str, str, bool]] = [
    # (naam, fragment dat in het run-blok komt, moet het falen)
    ("inputs", "${{ inputs.crate }}", True),
    ("event-inputs", "${{ github.event.inputs.crate }}", True),
    ("head-ref", "${{ github.head_ref }}", True),
    # A tag may be called `v1.2.$(id)`; ref_name hands over what is left after
    # the prefix. Codex, #1540.
    ("ref-name", "${{ github.ref_name }}", True),
    ("pr-title", "${{ github.event.pull_request.title }}", True),
    ("issue-body", "${{ github.event.issue.body }}", True),
    ("head-commit", "${{ github.event.head_commit.message }}", True),
    # Deze worden door de workflow zelf bepaald, niet door de aanroeper.
    ("matrix", "${{ matrix.target }}", False),
    ("run-number", "${{ github.run_number }}", False),
    ("sha", "${{ github.sha }}", False),
]


def main() -> int:
    fouten: list[str] = []

    for naam, fragment, moet_falen in GEVALLEN:
        with tempfile.TemporaryDirectory() as d:
            map_ = pathlib.Path(d)
            bestanden = vullen(12)
            bestanden["proef.yml"] = (
                "name: Proef\non: {workflow_dispatch: null}\njobs:\n"
                "  j:\n    runs-on: ubuntu-latest\n    steps:\n"
                f'      - run: echo "{fragment}"\n'
            )
            bouw(map_, bestanden)
            r = draai(map_)
            gefaald = r.returncode != 0
            if gefaald != moet_falen:
                fouten.append(
                    f"{naam}: expected {'a failure' if moet_falen else 'a pass'}, "
                    f"got exit {r.returncode}\n{r.stderr}"
                )

    # De schone vorm -- de fix zelf -- moet slagen.
    with tempfile.TemporaryDirectory() as d:
        map_ = pathlib.Path(d)
        bestanden = vullen(12)
        bestanden["schoon.yml"] = SCHOON
        bouw(map_, bestanden)
        r = draai(map_)
        if r.returncode != 0:
            fouten.append(f"the env: form is rejected:\n{r.stderr}")

    # Te weinig workflows: de ondergrens moet spreken, niet stilzwijgend slagen.
    with tempfile.TemporaryDirectory() as d:
        map_ = pathlib.Path(d)
        bouw(map_, {"een.yml": SCHOON})
        r = draai(map_)
        if r.returncode == 0 or "floor" not in r.stderr:
            fouten.append(f"the workflow floor does not speak: exit {r.returncode}\n{r.stderr}")

    # Genoeg workflows, te weinig run-blokken: de tweede ondergrens.
    with tempfile.TemporaryDirectory() as d:
        map_ = pathlib.Path(d)
        leeg = {
            f"leeg{n}.yml": f"name: L{n}\non: {{push: null}}\njobs:\n  j:\n"
            "    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n"
            for n in range(12)
        }
        bouw(map_, leeg)
        r = draai(map_)
        if r.returncode == 0 or "run:" not in r.stderr:
            fouten.append(f"the run-block floor does not speak: exit {r.returncode}\n{r.stderr}")

    # Twee `env:`-sleutels in een stap: PyYAML neemt stil de laatste, GitHub
    # weigert het bestand. Precies de fout die bij het herstellen van de tien
    # plekken werd gemaakt.
    with tempfile.TemporaryDirectory() as d:
        map_ = pathlib.Path(d)
        bestanden = vullen(12)
        bestanden["dubbel.yml"] = (
            "name: Dubbel\non: {workflow_dispatch: null}\njobs:\n"
            "  j:\n    runs-on: ubuntu-latest\n    steps:\n"
            "      - env:\n          A: '1'\n"
            "        run: echo \"$A$B\"\n"
            "        env:\n          B: '2'\n"
        )
        bouw(map_, bestanden)
        r = draai(map_)
        if r.returncode == 0:
            fouten.append("a step with two env: keys passes")

    # Geen map: de guard moet klagen, niet slagen.
    with tempfile.TemporaryDirectory() as d:
        r = draai(pathlib.Path(d))
        if r.returncode == 0:
            fouten.append("a missing .github/workflows passes")

    if fouten:
        print("[test-inputs-not-shell] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1

    print(
        f"[test-inputs-not-shell] OK: {len(GEVALLEN)} context(s), the env: form, "
        "both floors and a missing directory."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
