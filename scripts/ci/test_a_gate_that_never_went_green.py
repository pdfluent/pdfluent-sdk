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
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env, inside_the_sandbox

import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile

BEWAKER = pathlib.Path(__file__).with_name("a_gate_that_never_went_green.py")

# Read straight from the guard rather than repeating it. The first version
# listed the four names by hand; adding a fifth to BEKEND turned every case red
# at once, because each fixture then looked like a repository where a
# known-dead workflow had recovered. A test that has to be edited whenever the
# thing it tests grows is a test that will be edited wrongly.
_spec = importlib.util.spec_from_file_location("bewaker", BEWAKER)
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
BEKEND = tuple(_mod.BEKEND)

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
if "/commits?path=" in url:
    # A workflow marked _new has no commit on the default branch: the real API
    # answers an empty list for that, not an error.
    for wf in tabel["workflows"]:
        if wf.get("_new") and wf["path"] in url:
            print("[]"); raise SystemExit(0)
    # The guard dates each workflow from the default branch. A fixed date far
    # enough back that the run counts in the table are what decides each case.
    print(json.dumps([{"commit": {"committer": {"date": "2026-01-01T00:00:00Z"}}}]))
    raise SystemExit(0)
for wf in tabel["workflows"]:
    if f"/workflows/{wf['id']}/runs" in url:
        groen = "status=success" in url
        binnen = "created=" in url
        # Unscoped queries answer with the all-refs numbers, which is what the
        # real API does. A guard that forgets `branch=` therefore reads the
        # other-branch counts and the case below catches it.
        if "branch=" not in url:
            n = wf.get("_green_anyref", wf["_green_all"]) if groen else wf.get("_runs_anyref", wf["_runs_all"])
            print(json.dumps({"total_count": n, "workflow_runs": []})); raise SystemExit(0)
        # Per-conclusion, because the guard must add up only the conclusions
        # that judged something. A cancellation is not one of them.
        if "status=cancelled" in url:
            print(json.dumps({"total_count": wf.get("_cancelled", 0), "workflow_runs": []}))
            raise SystemExit(0)
        if "status=completed" in url:
            n = wf["_runs_in"] + wf.get("_cancelled", 0)
            print(json.dumps({"total_count": n, "workflow_runs": []})); raise SystemExit(0)
        if "status=failure" in url:
            n = max(0, wf["_runs_in"] - wf["_green_in"])
            print(json.dumps({"total_count": n, "workflow_runs": []})); raise SystemExit(0)
        if "status=timed_out" in url:
            print(json.dumps({"total_count": 0, "workflow_runs": []})); raise SystemExit(0)
        n = wf["_green_in"] if groen else wf["_runs_in"]
        if not binnen:
            n = wf["_green_all"] if groen else wf["_runs_all"]
        print(json.dumps({"total_count": n, "workflow_runs": []})); raise SystemExit(0)
print("stub gh: unexpected call: " + url, file=sys.stderr)
raise SystemExit(1)
"""


def schoon() -> dict[str, str]:
    """The environment without the caller's git, or anyone's GitHub token.

    GH_TOKEN sends the guard to the real API over urllib, which would make this
    suite pass or fail on the state of the repository that day and ignore the
    stub entirely. Stripping it is what makes the stub the only route.

    The fixture is a real repository and the guard reads it with `git log`. Run
    from .githooks/pre-push, GIT_DIR and GIT_INDEX_FILE are already exported and
    point at the actual checkout, so `git init` in a temporary directory is
    followed by a `git add` that exits 128 -- or worse, succeeds against the
    wrong repository. The test passed when run by hand and failed inside the
    hook, which is the only place it had a chance to be wrong.
    """
    return {k: v for k, v in os.environ.items()
            if not k.startswith("GIT_") and k not in ("GH_TOKEN", "GITHUB_TOKEN")}


def bouw(map_: pathlib.Path, workflows: list[dict]) -> None:
    """A real git repo with the workflow files, plus a stub gh on PATH."""
    wf = map_ / ".github" / "workflows"
    wf.mkdir(parents=True)
    for w in workflows:
        (wf / pathlib.Path(w["path"]).name).write_text(WERKSTROOM)

    env = {**sealed_env(identity=True, cwd=map_), "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
           "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t"}
    for cmd in (["git", "init", "-q"],
                ["git", "add", "--", ".github"],
                ["git", "commit", "-qm", "fixture"]):
        subprocess.run(cmd, cwd=map_, check=True, capture_output=True, env=env)

    tabel = map_ / "tabel.json"
    tabel.write_text(json.dumps({"workflows": workflows}))
    bin_ = map_ / "bin"
    bin_.mkdir()
    gh = bin_ / "gh"
    gh.write_text(STUB % str(tabel))
    gh.chmod(0o755)


def draai(map_: pathlib.Path, **extra: str) -> subprocess.CompletedProcess[str]:
    env = {**sealed_env(cwd=map_), "PATH": f"{map_ / 'bin'}{os.pathsep}{os.environ['PATH']}",
           **extra}
    return subprocess.run([sys.executable, str(BEWAKER)], cwd=map_,
                          capture_output=True, text=True, check=False, env=env)


def wf(naam, runs_in, green_in, runs_all=None, green_all=None, state="active",
       runs_anyref=None, green_anyref=None, cancelled=0, new=False):
    d = {"id": abs(hash(naam)) % 100000, "path": f".github/workflows/{naam}",
         "state": state, "_runs_in": runs_in, "_green_in": green_in,
         "_runs_all": runs_all if runs_all is not None else runs_in,
         "_green_all": green_all if green_all is not None else green_in}
    if runs_anyref is not None:
        d["_runs_anyref"] = runs_anyref
    if green_anyref is not None:
        d["_green_anyref"] = green_anyref
    d["_cancelled"] = cancelled
    if new:
        d["_new"] = True
    return d


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
    # ...but GitHub switching a scheduled workflow off after sixty quiet days is
    # nobody's decision, and it can happen once this repository is public. The
    # first version excused every non-active state and would have approved a
    # scheduled gate that had gone dark. (codex, #1610)
    ("disabled by inactivity is not an excuse",
     [wf("stil.yml", 40, 0, state="disabled_inactivity")], True),
    # The queries must be scoped to the default branch. Dead on master, green on
    # some other ref: a guard reading all refs calls this healthy, which is the
    # opposite of the truth rather than a gap in it.
    ("green on another branch does not rescue a workflow dead on master",
     [wf("tak.yml", 40, 0, runs_anyref=80, green_anyref=40)], True),
    # The baseline excuses by name, and only by name.
    ("a name in BEKEND is excused", [wf(BEKEND[0], 40, 0)], False),
    ("a name not in BEKEND beside one that is",
     [wf(BEKEND[0], 40, 0), wf("ander.yml", 40, 0)], True),
    # A baseline that only grows is a list of excuses.
    # One known-dead workflow recovers: the baseline must be made to shrink, or
    # it stops being a baseline and becomes a list of excuses.
    ("BEKEND must shrink when a known-dead one recovers",
     [wf(BEKEND[0], 40, 3)] + [wf(b, 40, 0) for b in BEKEND[1:]], True),
    # ...but "not judged" is not "recovered". Every entry becomes not-judged the
    # moment someone edits its file, because the window restarts empty. The
    # first version asked for BEKEND minus everything currently dead, so a
    # commit touching a known-dead workflow demanded its own baseline entry be
    # deleted -- which is how a baseline quietly empties itself.
    # Cancellations judged nothing. Four of them plus one real failure is one
    # run's worth of evidence, not five -- and on a repository with one runner
    # and `cancel-in-progress`, bursts of cancellations are the normal case.
    ("four cancellations and one failure is too few to judge",
     [wf("druk.yml", 1, 0, cancelled=4)], False),
    ("three real failures still count",
     [wf("echt.yml", 3, 0, cancelled=0)], True),
    ("a BEKEND entry with too few runs is not a recovery",
     [wf(BEKEND[0], 0, 0)] + [wf(b, 40, 0) for b in BEKEND[1:]], False),
    ("a BEKEND entry with one red run is not a recovery either",
     [wf(BEKEND[0], 1, 0)] + [wf(b, 40, 0) for b in BEKEND[1:]], False),
]


def main() -> int:
    fouten: list[str] = []
    for naam, workflows, moet_falen in GEVALLEN:
        # Every case carries the whole BEKEND set as still-dead, so the
        # both-directions check does not fire by accident on unrelated cases.
        namen = {w["path"].split("/")[-1] for w in workflows}
        vol = list(workflows) + [wf(b, 40, 0) for b in BEKEND if b not in namen]
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

    # A workflow that exists only on this branch has no commit on the default
    # branch, and GitHub answers `[]` for it. The guard once read that as "could
    # not date the file" and printed SKIPPED (not a pass) -- a failure under CI
    # -- so every pull request that added a workflow was red for having added
    # one. It must be named on stdout, and neither skipped nor failed, with CI
    # set as it is in the pipeline. (review of #1672)
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, [wf("vers.yml", 0, 0, new=True), wf("ok.yml", 40, 12)]
                + [wf(b, 40, 0) for b in BEKEND])
        r = draai(m, CI="1")
        if r.returncode != 0:
            fouten.append(f"a workflow new on this branch fails the guard: exit "
                          f"{r.returncode}\n      {r.stderr.strip()[:300]}")
        if "SKIPPED" in r.stderr:
            fouten.append(f"a workflow new on this branch is reported as a skip: "
                          f"{r.stderr.strip()[:300]}")
        if "vers.yml" not in r.stdout:
            fouten.append(f"a workflow new on this branch is not named: {r.stdout[-300:]}")

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
    print(f"[test-groen] OK: {len(GEVALLEN)} history shapes, the new-on-this-branch case "
          "and the unreachable-API case.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
