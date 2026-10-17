#!/usr/bin/env python3
"""pr_staleness.py carries gh's own words when it cannot look (#294).

Both callers of gh() -- the list call and the per-PR compare call -- with a
fake `gh` on PATH, so the assertion is about what the guard prints and not
about today's rate limit. Measured by hand on review of #1674; now measured here.
"""
from __future__ import annotations
import os, pathlib, subprocess, sys, tempfile

GUARD = pathlib.Path(__file__).resolve().with_name("pr_staleness.py")
RATE = "HTTP 403: API rate limit exceeded for user ID 1 (fake)"
FAKE = f'''#!/bin/sh
case "$2" in
  */pulls*) [ "$FAKE_GH_MODE" = listfail ] && {{ echo "{RATE}" >&2; exit 1; }}
    echo '[{{"number": 4242, "updated_at": "2026-09-02T00:00:00Z", "title": "x",
           "base": {{"ref": "master"}}, "head": {{"sha": "abc"}}}}]' ;;
  *) echo "{RATE}" >&2; exit 1 ;;
esac
'''
fails: list[str] = []


def run(mode: str, with_gh: bool) -> subprocess.CompletedProcess:
    with tempfile.TemporaryDirectory() as td:
        if with_gh:
            gh = pathlib.Path(td, "gh"); gh.write_text(FAKE); gh.chmod(0o755)
        env = dict(os.environ, PATH=td, FAKE_GH_MODE=mode)
        return subprocess.run([sys.executable, str(GUARD)], env=env,
                              capture_output=True, text=True)


def expect(what: str, ok: bool, detail: str) -> None:
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f" -- {detail[:200]}"))
    if not ok:
        fails.append(what)


print("pr_staleness quotes the tool")
r = run("", with_gh=False)
expect("gh missing: FAIL with the missing-binary reason",
       r.returncode == 1 and "`gh` is not installed" in r.stderr, r.stderr)
expect("  and not the old guess", "installed and authenticated" not in r.stderr, r.stderr)
r = run("listfail", with_gh=True)
expect("list call fails: gh's own line is quoted",
       r.returncode == 1 and RATE in r.stderr, r.stderr)
r = run("comparefail", with_gh=True)
expect("compare call fails: the PR line carries the reason",
       "#4242 could not be compared" in r.stdout and RATE in r.stdout, r.stdout + r.stderr)
expect("  and the guard itself does not crash", r.returncode in (0, 1), str(r.returncode))

# The blocking half has to be reachable from somewhere, or `return 1` is dead
# code and the non-blocking message promises a run nobody scheduled. That is
# exactly what it was until 03-09-2026. Asserted here rather than trusted,
# because the promise lives in one file and the run that keeps it in another.
# (T1 review of #1679)
#
# The run is looked for in EVERY workflow rather than in a file named here. It
# lived in nightly.yml until 15-09-2026 and moved out when the paid-compute lock
# left that workflow no trigger but `workflow_dispatch`; a test that names the
# file goes red on the move itself rather than on the thing it is about. What
# must hold is that ONE workflow carries all three -- the run, the blocking
# variable, and the weekly cron -- because three facts spread over three files
# are not a scheduled blocking run. (#358)
import pathlib as _pl
import re as _re
import yaml as _yaml

_dir = _pl.Path(__file__).resolve().parents[2] / ".github/workflows"
_carriers = []
for _wf in sorted(_dir.glob("*.yml")):
    _t = _wf.read_text(encoding="utf-8")
    if "scripts/ci/pr_staleness.py" not in _t:
        continue
    _blocking = _re.search(r"PR_STALENESS_BLOCKING:\s*'?1'?", _t) is not None
    try:
        _doc = _yaml.safe_load(_t) or {}
    except _yaml.YAMLError:
        _doc = {}
    # `on:` parses as the boolean True in YAML 1.1, which is why it is asked for
    # both ways; reading only "on" finds nothing and would pass this vacuously.
    _on = _doc.get("on", _doc.get(True, {}))
    _crons = [str(_s.get("cron", "")) for _s in (_on or {}).get("schedule", []) or []] \
        if isinstance(_on, dict) else []
    _weekly = any(_re.fullmatch(r"0 2 \* \* 0", _c) for _c in _crons)
    _carriers.append((_wf.name, _blocking, _weekly))

expect("some workflow runs pr_staleness.py",
       bool(_carriers), "no workflow names the guard")
expect("  with PR_STALENESS_BLOCKING=1, so `return 1` is reachable",
       any(b for _, b, _ in _carriers), f"found {_carriers}")
expect("  on a weekly schedule in that same workflow, so it is the run the message names",
       any(b and w for _, b, w in _carriers), f"found {_carriers}")

print(f"\n  8 assertion(s) ran, {len(fails)} failure(s)")
raise SystemExit(1 if fails else 0)
