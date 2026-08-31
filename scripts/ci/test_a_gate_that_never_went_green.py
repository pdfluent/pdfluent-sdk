#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for a_gate_that_never_went_green.py, without the network.

The guard it tests reads GitHub. A test that also read GitHub would be green or
red depending on the state of the repository that day, which is not a test. So
a stub `gh` is put in front of it on PATH, answering from a table the case
writes -- and the fixture is a real git repository, because the guard's window
is "runs since the workflow file last changed" and that date comes from git.

The case that matters is the third one. docs-drift-guard.yml and
wasm-surface-guard.yml had 58 and 93 green runs behind them when they died on
28-08-2026, so a guard asking "has it ever been green" would have called them
healthy while they were failing every single run (#290). Green-in-the-past must
not excuse red-since-the-file-changed.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import tempfile

BEWAKER = pathlib.Path(__file__).with_name("a_gate_that_never_went_green.py")

WERKSTROOM = """\
name: Proef
on:
  workflow_dispatch:
jobs:
  j:
    runs-on: [self-hosted, xfa-fast]
    steps:
      - run: echo hi
"""

# A `gh` that answers from a JSON table keyed by "has this URL got ?status=success
# in it". Anything it is not asked about is an error, so a guard that starts
# asking different questions fails loudly instead of silently passing.
STUB = """\
#!/usr/bin/env python3
import json, sys, pathlib
tabel = json.loads(pathlib.Path(%r).read_text())
url = sys.argv[-1]
if "actions/workflows?" in url:
    print(json.dumps({"workflows": tabel["workflows"]})); raise SystemExit(0)
for wf in tabel["workflows"]:
    if f"/workflows/{wf['id']}/runs" in url:
        groen = "status=success" in url
        binnen = "created=" in url
        n = wf["_green_in"] if groen else wf["_runs_in"]
        if not binnen:
            n = wf["_green_all"] if groen else wf["_runs_all"]
        print(json.dumps({"total_count": n, "workflow_runs": []})); raise SystemExit(0)
print("stub gh: unexpected call: " + url, file=sys.stderr)
raise SystemExit(1)
"""


def bouw(map_: pathlib.Path, workflows: list[dict]) -> None:
    """A real git repo with the workflow files, plus a stub gh on PATH."""
    wf = map_ / ".github" / "workflows"
    wf.mkdir(parents=True)
    for w in workflows:
        (wf / pathlib.Path(w["path"]).name).write_text(WERKSTROOM)

    env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
    for cmd in (["git", "init", "-q"], ["git", "add", "-A"],
                ["git", "commit", "-qm", "fixture"]):
        subprocess.run(cmd, cwd=map_, check=True, capture_output=True, env=env)

    tabel = map_ / "tabel.json"
    tabel.write_text(json.dumps({"workflows": workflows}))
    bin_ = map_ / "bin"
    bin_.mkdir()
    gh = bin_ / "gh"
    gh.write_text(STUB % str(tabel))
    gh.chmod(0o755)


def draai(map_: pathlib.Path) -> subprocess.CompletedProcess[str]:
    env = {**os.environ, "PATH": f"{map_ / 'bin'}{os.pathsep}{os.environ['PATH']}"}
    return subprocess.run([sys.executable, str(BEWAKER)], cwd=map_,
                          capture_output=True, text=True, check=False, env=env)


def wf(naam, runs_in, green_in, runs_all=None, green_all=None, state="active"):
    return {"id": abs(hash(naam)) % 100000, "path": f".github/workflows/{naam}",
            "state": state, "_runs_in": runs_in, "_green_in": green_in,
            "_runs_all": runs_all if runs_all is not None else runs_in,
            "_green_all": green_all if green_all is not None else green_in}


GEVALLEN = [
    ("a healthy workflow", [wf("ok.yml", 40, 12)], False),
    ("red on every run since the file changed", [wf("dood.yml", 40, 0)], True),
    # The shape that made "has it ever been green" the wrong question: months of
    # green, then dead the day the file broke.
    ("green 93 times in the past, none since the file changed",
     [wf("wasm-surface-guard.yml", 169, 0, runs_all=345, green_all=93)], True),
    # A workflow touched an hour ago has not had a chance yet. Failing it would
    # fail the pull request that fixes it, which is how a guard gets removed.
    ("just fixed, one red run so far", [wf("net.yml", 1, 0)], False),
    ("just added, no runs at all", [wf("nieuw.yml", 0, 0)], False),
    # Off on purpose is not the same as broken.
    ("disabled on purpose", [wf("uit.yml", 40, 0, state="disabled_manually")], False),
    # The baseline excuses by name, and only by name.
    ("a name in BEKEND is excused", [wf("fuzz.yml", 40, 0)], False),
    ("a name not in BEKEND beside one that is",
     [wf("fuzz.yml", 40, 0), wf("ander.yml", 40, 0)], True),
    # A baseline that only grows is a list of excuses.
    ("BEKEND must shrink when a known-dead one recovers",
     [wf("fuzz.yml", 40, 3), wf("node-bindings.yml", 40, 0),
      wf("security-audit.yml", 40, 0), wf("enterprise-acceptance.yml", 40, 0)], True),
]


def main() -> int:
    fouten: list[str] = []
    for naam, workflows, moet_falen in GEVALLEN:
        # Every case carries the whole BEKEND set as still-dead, so the
        # both-directions check does not fire by accident on unrelated cases.
        namen = {w["path"].split("/")[-1] for w in workflows}
        vol = list(workflows) + [
            wf(b, 40, 0) for b in
            ("fuzz.yml", "node-bindings.yml", "security-audit.yml",
             "enterprise-acceptance.yml") if b not in namen]
        with tempfile.TemporaryDirectory() as d:
            m = pathlib.Path(d)
            bouw(m, vol)
            r = draai(m)
            gefaald = r.returncode != 0
            if gefaald != moet_falen:
                fouten.append(
                    f"{naam}: expected {'a failure' if moet_falen else 'a pass'}, got "
                    f"exit {r.returncode}\n      {r.stdout.strip()[-300:]}\n"
                    f"      {r.stderr.strip()[:300]}")

    # No `gh` at all must announce itself, not pass quietly.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, [wf("ok.yml", 40, 12)])
        (m / "bin" / "gh").write_text("#!/bin/sh\nexit 1\n")
        r = draai(m)
        if "SKIPPED (not a pass)" not in r.stderr:
            fouten.append(f"an unreachable API does not announce itself: {r.stderr[:200]}")

    if fouten:
        print("[test-groen] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"[test-groen] OK: {len(GEVALLEN)} history shapes and the unreachable-API case.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
