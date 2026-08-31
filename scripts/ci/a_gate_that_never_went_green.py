#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A check that has not been green since it was last touched is not a check.

Nothing in this repository was watching whether a workflow ever succeeds. Three
of them stopped starting at all on 28-08-2026 and it took three days and a
person reading the checks list to notice, because the only thing that changes
when a gate dies is that its cross was already there yesterday (#290).

The counts on 31-08-2026, over every run GitHub still held:

    bindings.yml                1008 runs      0 green
    enterprise-acceptance.yml     43 runs      0 green
    docs-drift-guard.yml         324 runs     58 green, none since 28-08
    wasm-surface-guard.yml       345 runs     93 green, none since 28-08
    avrt.yml                     219 runs     42 green, none since 07-05

Two different faults, and the second is why "has it ever been green" is not the
question. docs-drift-guard and wasm-surface-guard had both been green for
months; they died the day `env:` was orphaned under them, and a guard asking
only about all of history would have called them healthy for another year.

So the window is runs since the workflow file itself last changed. Runs older
than the file tested a different file and cannot say anything about this one.
That also keeps the guard from shouting at a workflow that was fixed an hour
ago and has not had a chance to run yet -- that state is reported, by name,
rather than either failed or quietly passed.

Needs the network. When it cannot reach GitHub it says SKIPPED (not a pass)
rather than returning a green nobody checked.

# NO-FLOOR: the run history comes from a live source and the workflow list from
# disk. Neither can quietly shrink -- an unreachable API is announced, and an
# empty workflow directory is a failure.
"""

from __future__ import annotations

import datetime as dt
import json
import os
import pathlib
import subprocess
import sys
import urllib.error
import urllib.request

FLOWS = pathlib.Path(".github/workflows")

# Below this many runs since the file changed, one failure is an anecdote and a
# brand-new workflow would be failed by its own first red run. Those are named
# in the report instead.
GENOEG = 3

# Known dead, with the reason and the issue. Checked in both directions: a
# baseline that only grows is a list of excuses, and one that only shrinks
# stops noticing when something dies.
BEKEND = {
    # Found by this guard on its own pull request, which is the strongest thing
    # it has said so far: the workflow it lives in is itself a dead gate.
    # `orchestration-guard` aborts on step four -- ci-ephemeral's `reap` job on
    # the persistent runner (#267) -- and has done on every run for weeks. Not
    # fixable from here; #1601 and #1605 both carry the repair and are open.
    # This entry comes out the day ci.yml goes green, and the both-directions
    # check below is what will insist on it.
    "ci.yml": "orchestration-guard aborts on step four since 28-08-2026 (#267; repair open in #1601/#1605)",
    "enterprise-acceptance.yml": "red on every run since 28-08-2026; found by this guard, not diagnosed (#290)",
    "fuzz.yml": "red on every run since 28-08-2026; found by this guard, not diagnosed (#290)",
    "node-bindings.yml": "red on every run since 28-08-2026; found by this guard, not diagnosed (#290)",
    "security-audit.yml": "red on every run since 28-08-2026; found by this guard, not diagnosed (#290)",
}

# A file that has not changed in this long and still has no runs is not new; it
# has stopped being triggered. bindings.yml is the case: 1008 runs, the last of
# them on 27-04-2026, and nothing since. Reported rather than failed -- why a
# workflow stopped being reached is #276 and #288 territory, not this guard's.
STIL_NA_DAGEN = 30


REPO = "jasperdew/xfa-native-rust"

# Why the last API call failed, so a skip can name its cause instead of being a
# shrug. A skip that does not say why is only marginally better than a silent
# one: you still cannot act on it.
REDEN: list[str] = []


def gh(pad: str):
    """One GitHub API call, by whichever route this machine has.

    `gh` is not installed on the desktop runner. The first version of this
    guard used it anyway, and on the pull request that introduced it the step
    printed

        SKIPPED (not a pass): could not read the workflow list from GitHub

    on a green job -- announcing itself, and still measuring nothing, which is
    the exact shape #290 is about. Passing GH_TOKEN was not enough; there is no
    binary to pass it to.

    So: the token over plain HTTP when there is one, which is the case in CI,
    and `gh` otherwise, which is the case on a workstation where the token
    lives in a keyring. urllib is the same route scripts/ci/mr_staleness.py
    takes for the same reason.
    """
    pad = pad.replace("{owner}/{repo}", REPO)
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        req = urllib.request.Request(
            f"https://api.github.com/{pad.lstrip('/')}",
            headers={"Authorization": f"Bearer {token}",
                     "Accept": "application/vnd.github+json",
                     "X-GitHub-Api-Version": "2022-11-28",
                     "User-Agent": "pdfluent-ci-guard"})
        try:
            with urllib.request.urlopen(req, timeout=60) as fh:
                return json.loads(fh.read().decode())
        except urllib.error.HTTPError as fout:
            # The status, not just "it did not work". A 403 here means the
            # workflow token lacks actions:read and the workflow needs a
            # `permissions:` block; a 404 on a private repository means the
            # same thing wearing a different number. Guessing between those
            # costs a fifteen-minute push each time.
            REDEN.append(f"HTTP {fout.code} {fout.reason} on {pad.split('?')[0]}")
            return None
        except (urllib.error.URLError, OSError, json.JSONDecodeError) as fout:
            REDEN.append(f"{type(fout).__name__}: {fout}")
            return None

    try:
        r = subprocess.run(["gh", "api", pad], capture_output=True, text=True,
                           check=False, timeout=120, stdin=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        return None
    if r.returncode != 0:
        return None
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return None


def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, and a subprocess
    inherits them. A git command meant for one directory then operates on
    whatever those point at. On 25-08-2026 that put `core.bare = true` on this
    repository and stopped all thirty worktrees. Same helper as
    scripts/ci/mr_staleness.py, and scripts/ci/no_test_can_touch_the_real_repo.py
    is what insists on it.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def veranderd_op(pad: pathlib.Path) -> dt.datetime | None:
    """When this workflow file last changed, in UTC."""
    try:
        r = subprocess.run(["git", "log", "-1", "--format=%cI", "--", str(pad)],
                           capture_output=True, text=True, check=False, timeout=60,
                           stdin=subprocess.DEVNULL, env=schone_omgeving())
    except (OSError, subprocess.SubprocessError):
        return None
    uit = r.stdout.strip()
    if r.returncode != 0 or not uit:
        return None
    try:
        return dt.datetime.fromisoformat(uit).astimezone(dt.timezone.utc)
    except ValueError:
        return None


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[groen] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    op_schijf = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    if not op_schijf:
        print("[groen] FATAL: no workflow files at all", file=sys.stderr)
        return 1

    lijst = gh("repos/{owner}/{repo}/actions/workflows?per_page=100")
    if lijst is None or "workflows" not in lijst:
        waarom = REDEN[-1] if REDEN else ("no GH_TOKEN and no usable `gh`")
        print("SKIPPED (not a pass): could not read the workflow list from GitHub, so "
              f"no gate was checked against its own history. Cause: {waarom}.",
              file=sys.stderr)
        return 0

    bij_pad = {w["path"]: w for w in lijst["workflows"]}

    dood, jong, stil, uitgezet, gezond = [], [], [], [], 0
    hersteld_namen: set[str] = set()
    nu = dt.datetime.now(dt.timezone.utc)

    for pad in op_schijf:
        sleutel = f".github/workflows/{pad.name}"
        wf = bij_pad.get(sleutel)
        if wf is None:
            # Never registered: GitHub has not seen this file on the default
            # branch yet. Added in this branch, most likely.
            jong.append((pad.name, "not registered with GitHub yet"))
            continue
        if wf.get("state") != "active":
            uitgezet.append((pad.name, wf.get("state")))
            continue

        sinds = veranderd_op(pad)
        if sinds is None:
            jong.append((pad.name, "no commit date for the file"))
            continue
        vanaf = sinds.strftime("%Y-%m-%dT%H:%M:%SZ")

        alle = gh(f"repos/{{owner}}/{{repo}}/actions/workflows/{wf['id']}/runs"
                  f"?per_page=1&created=%3E{vanaf}")
        groen = gh(f"repos/{{owner}}/{{repo}}/actions/workflows/{wf['id']}/runs"
                   f"?per_page=1&status=success&created=%3E{vanaf}")
        if alle is None or groen is None:
            waarom = REDEN[-1] if REDEN else "unknown"
            print(f"SKIPPED (not a pass): could not read the run history of "
                  f"{pad.name}. Cause: {waarom}.", file=sys.stderr)
            return 0

        n, g = alle["total_count"], groen["total_count"]
        if g > 0:
            gezond += 1
            hersteld_namen.add(pad.name)
        elif n == 0 and (nu - sinds).days > STIL_NA_DAGEN:
            stil.append((pad.name, sinds, (nu - sinds).days))
        elif n < GENOEG:
            jong.append((pad.name, f"{n} run(s) since the file changed on "
                                   f"{sinds:%d-%m-%Y}; too few to judge"))
        else:
            dood.append((pad.name, n, sinds))

    for naam, reden in sorted(uitgezet):
        print(f"[groen] {naam}: {reden}, so nothing is claimed for it")
    for naam, reden in sorted(jong):
        print(f"[groen] {naam}: not judged -- {reden}")
    for naam, sinds, dagen in sorted(stil):
        print(f"[groen] {naam}: SILENT -- no run at all since the file changed "
              f"{dagen} days ago ({sinds:%d-%m-%Y}). Not judged here; a workflow "
              f"nothing triggers is #276/#288.")
    print(f"[groen] {gezond} workflow(s) green since their file last changed; "
          f"{len(dood)} not.")

    namen = {naam for naam, _, _ in dood}
    nieuw = sorted(namen - set(BEKEND))
    if nieuw:
        print(file=sys.stderr)
        print(f"[groen] FATAL: {len(nieuw)} workflow(s) have not been green once since "
              "their own file last changed:", file=sys.stderr)
        for naam, n, sinds in sorted(dood):
            if naam in nieuw:
                print(f"  {naam}: {n} run(s) since {sinds:%d-%m-%Y}, none green",
                      file=sys.stderr)
        print(
            "\nA cross that was already there yesterday is not read as news, which is "
            "how three of these went unnoticed for three days. Fix it, or switch the "
            "workflow off with the reason in the file, or name it in BEKEND with the "
            "reason and the issue. An honest gap beats a red check nobody can act on. "
            "(#290)",
            file=sys.stderr,
        )
        return 1

    # Only a workflow that has actually gone green may be demanded out of
    # BEKEND. The first version asked for `set(BEKEND) - namen`, which counts
    # "not judged" as "recovered" -- and every entry becomes not-judged the
    # moment somebody edits its file, because the window restarts empty. It
    # fired on ci.yml one commit after ci.yml was added to BEKEND, for a commit
    # that touched ci.yml. A baseline that empties itself when you edit the file
    # is not a baseline.
    hersteld = sorted(set(BEKEND) & hersteld_namen)
    if hersteld:
        print(file=sys.stderr)
        print(f"[groen] FATAL: {len(hersteld)} workflow(s) in BEKEND have gone green "
              f"since their file last changed: {', '.join(hersteld)}.", file=sys.stderr)
        print("Remove them from BEKEND, so the next one to die is still caught.",
              file=sys.stderr)
        return 1

    weg = sorted(n for n in BEKEND
                 if n not in {p.name for p in op_schijf})
    if weg:
        print(file=sys.stderr)
        print(f"[groen] FATAL: {len(weg)} name(s) in BEKEND no longer exist: "
              f"{', '.join(weg)}.", file=sys.stderr)
        print("Remove them; a baseline naming files that are gone stops being read.",
              file=sys.stderr)
        return 1

    if BEKEND:
        print(f"[groen] {len(BEKEND)} known-dead workflow(s) still dead, as recorded: "
              + "; ".join(f"{k} ({v})" for k, v in sorted(BEKEND.items())))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
