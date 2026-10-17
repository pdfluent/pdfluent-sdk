#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Prove the preview publish gate holds the ratchet and nothing else (#574).

Owner decision 2026-10-06: the structured extraction ships in 1.1.0 as
PREVIEW. Rule #574 ("every axis at least the reference") no longer blocks
publication; it is the condition for the "stable" label and for any parity
claim. So `--release` -- the spelling every publish path calls -- prints the
parity table under a PREVIEW header and exits on the RATCHET alone, and
`--stable` is the old `--release`: it exits unless every axis is at parity,
and no publish path calls it.

Each case is red on the old behaviour and green after:

  below the reference, not below     `--release` exits 0 where it exited 1,
  the last master round               `--stable` exits 1, and the table is
                                      headed PREVIEW.
  a mean that fell past the           `--release` exits 1, about the ratchet:
  tolerance over the same pages       the count-held rule still binds the
                                      publish path.
  a count that fell                   `--release` exits 1: the ratchet's
                                      strictest rule still binds it too.
  the preflight itself                with the gate stubbed to 0/1, the script
                                      prints the new sentences and exits
                                      accordingly.

The two regression cases assert the PREVIEW header as well as the RATCHET
verdict, which is what makes them red on the old tree: the old `--release`
refused those runs too, but under no preview header.

Exit codes:
  0  the publish gate holds the ratchet and the label holds parity
  1  it does not
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GATE = "scripts/ci/loop/structured_parity.py"
REFERENCE = "benchmarks/structured/PARITY.toml"
RUNS = "benchmarks/runs/structured"
PREFLIGHT = "scripts/release/the_structured_extraction_is_at_parity.sh"

PREVIEW_HEADER = ("PREVIEW: parity with the reference is NOT required to "
                  "publish (owner 2026-10-06); it is required for the "
                  "stable label")
PUBLISHED = ("the structured extraction is not below the last master round "
             "(preview; parity is required for stable only)")
REGRESSED = ("the structured extraction REGRESSED against the last master "
             "round; nothing was audited and nothing published")

# The reference of the fixture: round numbers, so that a reader of a failure
# can do the arithmetic in their head.
AXES = {"text": 0.600, "headings": 0.800, "lists": 0.700, "tables": 0.500,
        "order": 0.660, "order_geometric": 0.870}

FIXTURE = """
[meta]
tolerance = 0.005
ratchet_tolerance = 0.001
speed_share = 0.01

[sets.doclaynet]
run_id = "structured-doclaynet-20260101T000000Z"
tool = "docling"
tool_name = "Docling"
tool_version = "2.126.0"
pages = 200
half = "tuning"
start_up_amortised = true
ms_per_page = 2000.0

[sets.doclaynet.axes]
""" + "\n".join(f"{axis} = {value}" for axis, value in AXES.items()) + "\n"

# What the preflight calls, replaced by a verdict on demand: the script's own
# words and exit codes are what this file holds, not the gate's.
STUB = """#!/usr/bin/env python3
import os
import sys
print("stub: a parity table and a ratchet table would be here")
sys.exit(int(os.environ.get("STUB_GATE_EXIT", "0")))
"""


def run_file(run_id: str, axes: dict, pages: dict | None = None,
             ms: float = 5.0) -> dict:
    pages = pages or {}
    return {
        "run_id": run_id, "set": "doclaynet", "half": "tuning",
        "pages_measured": 200, "set_version": "fixture", "set_licence": "fixture",
        "machine": {"cpus": 8, "platform": "fixture"},
        "tools": {"pdfluent": {
            "name": "PDFluent", "version": "fixture", "ms_per_page": ms,
            "summary": {axis: {"mean": value, "pages": pages.get(axis, 100)}
                        for axis, value in axes.items()},
        }},
    }


def build_sandbox(root: pathlib.Path) -> None:
    target = root / GATE
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(REPO / GATE, target)
    (root / REFERENCE).parent.mkdir(parents=True, exist_ok=True)
    (root / REFERENCE).write_text(FIXTURE)
    (root / RUNS).mkdir(parents=True, exist_ok=True)


def write_runs(root: pathlib.Path, runs: list[dict]) -> None:
    place = root / RUNS
    for old in place.glob("*.json"):
        old.unlink()
    for run in runs:
        (place / f"{run['run_id']}.json").write_text(json.dumps(run, indent=1))


def gate(root: pathlib.Path, *flags: str) -> tuple[int, str]:
    done = subprocess.run([sys.executable, str(root / GATE), *flags],
                          capture_output=True, text=True, check=False, cwd=root)
    return done.returncode, done.stdout + done.stderr


def preflight(root: pathlib.Path, gate_exit: int) -> tuple[int, str]:
    script = root / PREFLIGHT
    script.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(REPO / PREFLIGHT, script)
    stub = root / GATE
    stub.parent.mkdir(parents=True, exist_ok=True)
    stub.write_text(STUB)
    env = dict(os.environ, STUB_GATE_EXIT=str(gate_exit))
    done = subprocess.run(["bash", str(script)], capture_output=True,
                          text=True, check=False, cwd=root, env=env)
    return done.returncode, done.stdout + done.stderr


def main() -> int:
    failures: list[str] = []

    def fails(what: str, code: int, output: str, expect: str) -> None:
        if code == 0:
            failures.append(f"{what}: the gate stayed green\n{output}")
        elif expect and expect not in output:
            failures.append(f"{what}: red, but not about {expect!r}:\n{output}")

    def passes(what: str, code: int, output: str) -> None:
        if code != 0:
            failures.append(f"{what}: the gate was red\n{output}")

    def headed(what: str, output: str) -> None:
        if PREVIEW_HEADER not in output:
            failures.append(f"{what}: no PREVIEW header on the table:\n{output}")

    with tempfile.TemporaryDirectory(prefix="structured-preview-test-") as tmp:
        root = pathlib.Path(tmp)
        build_sandbox(root)
        low = dict(AXES, headings=AXES["headings"] - 0.05)

        # 1. below the reference, but not below the last master round ------
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", low),
            run_file("structured-doclaynet-20260103T000000Z", low)])
        code, output = gate(root, "--release")
        passes("a run below the reference but not below the last master "
               "round, for a publish", code, output)
        headed("the same run", output)
        code, output = gate(root, "--check")
        passes("the push gate on the same runs", code, output)
        code, output = gate(root, "--stable")
        fails("a run five points below the reference, for the stable label",
              code, output, "PARITY")

        # 1b. and with no earlier round there is no ratchet to fail --------
        write_runs(root, [run_file("structured-doclaynet-20260102T000000Z",
                                   low)])
        code, output = gate(root, "--release")
        passes("a first round below the reference, for a publish", code,
               output)
        headed("the first round", output)

        # 2. a mean that fell past the tolerance over the same pages -------
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", low),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(low, lists=low["lists"] - 0.01))])
        code, output = gate(root, "--release")
        fails("a mean that fell a point over the same pages, for a publish",
              code, output, "RATCHET")
        headed("the regressed run", output)

        # 3. a count that fell, with a better mean --------------------------
        write_runs(root, [
            run_file("structured-doclaynet-20260102T000000Z", low),
            run_file("structured-doclaynet-20260103T000000Z",
                     dict(low, lists=0.99), pages={"lists": 10})])
        code, output = gate(root, "--release")
        fails("an axis scored on fewer pages with a better mean, for a "
              "publish", code, output, "RATCHET")
        headed("the narrowed run", output)

        # 4. the preflight itself, with the gate stubbed --------------------
        code, output = preflight(root, 0)
        passes("the preflight when the gate holds", code, output)
        if PUBLISHED not in output:
            failures.append("the preflight held and did not say so:\n"
                            + output)
        code, output = preflight(root, 1)
        fails("the preflight when the gate refuses", code, output, REGRESSED)
        if "none of it is released" in output:
            failures.append("the preflight still speaks the 20-09 sentence:\n"
                            + output)

    if failures:
        print(f"The preview gate: {len(failures)} problem(s)\n", file=sys.stderr)
        for line in failures:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("✓ the preview publish gate holds the ratchet under a PREVIEW "
          "header, and parity is the stable label's")
    return 0


if __name__ == "__main__":
    sys.exit(main())
