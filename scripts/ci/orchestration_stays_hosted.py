#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Nothing from an unmerged branch may run on the persistent desktop runner.

`gh workflow run --ref <branch>` executes the workflow definition from that
branch. So a job pinned to the persistent `[self-hosted, xfa-fast]` host runs
whatever steps that branch contains -- on the machine that holds the corpus, the
warm cargo cache and the runner registration token. The rule is that pre-merge
code stays on isolated runners, and a workflow that dispatches by ref is exactly
where that rule gets bypassed.

Found by Codex review on PR #1535, after that PR moved orchestration onto
xfa-fast to save 10-20s of hosted time per run. The saving was real; the
exposure it bought back is not bounded, and the person most likely to trigger it
was the one dispatching feature branches every few minutes.

The second half is availability, not security. The orphan sweep exists to clean
up Hetzner instances when a workflow fails to. Run it on the machine whose
failure it is catching and it cannot do that: a desktop that loses power after
creating an instance takes the sweep with it, and the orphan keeps billing until
someone notices.

WHAT THIS ALLOWS

A self-hosted runner is fine for workflows that only ever run on the default
branch, because there is no unmerged code to smuggle in. `ci.yml` does that on
push to master. What is refused is a self-hosted job in a workflow that can be
dispatched or triggered against an arbitrary ref.

Exit codes:
    0  no ref-dispatchable workflow puts a job on a persistent runner
    1  one does, or the scan found nothing to look at
"""

from __future__ import annotations

# FLOOR: workflows inspected >= 10 -- the repository has far more, and a scan
# that reads a handful has lost its glob rather than found a clean tree.
import re
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parent.parent.parent
FLOWS = REPO / ".github" / "workflows"
FLOOR = 10

# `self-hosted` is implied by a custom label but does not have to be written:
# `runs-on: [xfa-fast]` routes to the same machine and contains neither word.
# Codex caught this; matching the literal string alone would have let the next
# job through.
PERSISTENT_LABELS = ("self-hosted", "xfa-fast")


# The safe shape: pick the persistent runner only for a push, fall back to a
# hosted one otherwise.
#
#   runs-on: ${{ github.event_name == 'push' && fromJSON('["self-hosted","xfa-fast"]')
#              || 'ubuntu-latest' }}
#
# A push to the default branch runs code that is already merged; the pull
# request that preceded it ran hosted. Reading this as a violation would flag
# ci.yml and crash-guard.yml, which are deliberately built this way -- and a
# guard that cries wolf is one that gets switched off.
# A job that refuses to run unless the ref is the default branch.
#
# Accepted here, with a caveat that belongs in writing: the `if:` lives in the
# workflow file, and a dispatch by ref runs that branch's copy of it. A branch
# that deletes the guard is not stopped by the guard. Codex raised this on
# #1538 and it is correct.
#
# What it stops is the accident, which is the failure that actually occurs. The
# real boundary is repository write access -- whoever can dispatch could push to
# the default branch instead. So this check reports the shape being right on the
# default branch, where it can be relied on, and not a wall against a hostile
# branch. Claiming otherwise would make the next reader trust it further than it
# goes.
#
# The comparison is against the full ref, not `ref_name`: that strips both
# refs/heads/ and refs/tags/, so a TAG named master would satisfy a short-name
# check. Codex caught that too.
REF_VASTGEZET = re.compile(
    r"github\.ref\s*==\s*"
    r"(format\('refs/heads/\{0\}',\s*github\.event\.repository\.default_branch\)"
    r"|'refs/heads/(master|main)')")

ALLEEN_BIJ_PUSH = re.compile(
    r"github\.event_name\s*==\s*'push'\s*&&.*?\|\|\s*'[^']*ubuntu", re.S)


def op_blijvende_runner(runs_on) -> bool:
    tekst = str(runs_on)
    if not any(l in tekst for l in PERSISTENT_LABELS):
        return False
    return not ALLEEN_BIJ_PUSH.search(tekst)

# Eight jobs that already had this exposure before the ephemeral workflows
# existed. Recorded so the count cannot grow while they are dealt with
# separately (see the issue linked from each), not forgiven: every one of them
# runs branch-chosen workflow code on the persistent desktop.
#
# publish-crates.yml was the sharpest of them and is fixed: its two jobs now
# refuse to run off the default branch, and the crates.io token moved from
# workflow-level env -- where every step of every job could read it -- onto the
# single step that logs in.
BASELINE = {
    ("bench.yml", "benchmark"),
    # Found only after Codex pointed out that a pull_request branch filter names
    # the base, not the source. Runs on [self-hosted, xfa-corpus] -- a second
    # persistent machine -- so ubuntu-latest is not the fix here; restricting the
    # trigger is.
    ("crash-guard.yml", "crash-guard"),
    ("gate-ci.yml", "gate"),
    ("wasm-gate.yml", "wasm-gate"),
}

# Triggers that let a caller choose the ref, and therefore the workflow body.
#
# `pull_request` belongs here, which is not obvious: its `branches:` filter
# selects the BASE branch, so `branches: [master]` means "PRs targeting master"
# and the workflow still runs from the merge commit -- source branch included.
# Codex caught that; the first version read the filter as proof the code was
# already merged, which is the opposite of what a pull request is.
# `schedule` is deliberately absent. A scheduled run always takes the workflow
# file from the default branch, so the caller cannot choose the body -- unlike
# `workflow_dispatch`, where the ref is a field on the form.
REF_CHOSEN_BY_CALLER = {"workflow_dispatch", "workflow_call",
                        "pull_request", "pull_request_target", "repository_dispatch"}


class GeenDubbeleSleutels(yaml.SafeLoader):
    """PyYAML takes the last of two identical keys; GitHub rejects the file.

    That gap cost a run: an inserted `inputs:` block sat next to the existing
    one, `yaml.safe_load` accepted it silently, and GitHub answered "this run
    likely failed because of a workflow file issue" with no line number. A
    validator that is more permissive than the thing it validates is not a
    validator.
    """


def _geen_dubbele(loader, node, deep=False):
    gezien = {}
    for k, v in node.value:
        sleutel = loader.construct_object(k, deep=deep)
        if sleutel in gezien:
            raise yaml.YAMLError(f"duplicate key {sleutel!r}")
        gezien[sleutel] = loader.construct_object(v, deep=deep)
    return gezien


GeenDubbeleSleutels.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _geen_dubbele)


def triggers(doc: dict) -> dict:
    # PyYAML reads a bare `on:` key as the boolean True.
    return doc.get(True) or doc.get("on") or {}


def branch_locked(trigger_block) -> bool:
    """True when every trigger is pinned to the default branch.

    Two qualify. `push` with a `branches: [master]` filter runs code that is,
    by definition, already on master. `schedule` needs no filter at all: GitHub
    only ever runs a scheduled workflow from the default branch, so there is no
    ref for a caller to choose.

    A `pull_request` does not qualify, however its filter reads: `branches:`
    names the *target*, and the body comes from the merge commit, source branch
    included. Neither does `push` with only a `tags:` filter -- a tag can point
    at any commit, including one that never reached master.
    """
    if not isinstance(trigger_block, dict):
        return False
    for naam, waarde in trigger_block.items():
        if naam in REF_CHOSEN_BY_CALLER:
            return False
        if naam == "schedule":
            continue
        takken = (waarde or {}).get("branches") if isinstance(waarde, dict) else None
        if not takken or set(takken) - {"master", "main"}:
            return False
    return True


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[orchestration] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    bekeken, overtredingen, bekend = 0, [], []
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        try:
            doc = yaml.load(pad.read_text(), GeenDubbeleSleutels) or {}
        except yaml.YAMLError as e:
            print(f"[orchestration] FATAL: {pad.name} will not parse: {e}", file=sys.stderr)
            return 1
        bekeken += 1
        tr = triggers(doc)
        if branch_locked(tr):
            continue
        for naam, job in (doc.get("jobs") or {}).items():
            if not op_blijvende_runner(job.get("runs-on", "")):
                continue
            if REF_VASTGEZET.search(str(job.get("if", ""))):
                # Pinned to the default branch: a dispatch from a feature branch
                # skips the job entirely, so there is no unmerged code to run.
                continue
            if (pad.name, naam) in BASELINE:
                bekend.append((pad.name, naam))
                continue
            overtredingen.append((pad.name, naam, sorted(tr) if isinstance(tr, dict) else tr))

    if bekeken < FLOOR:  # FLOOR
        print(f"[orchestration] FATAL: {bekeken} workflow(s) read, floor is {FLOOR}. The glob "
              f"lost its files, and an empty scan reports a clean tree.", file=sys.stderr)
        return 1

    print(f"[orchestration] {bekeken} workflow(s) inspected, {len(bekend)} known exposure(s)")

    # A baseline that stops matching reality is worse than none: it says nine
    # when there are eight, and the one that left took its reason with it.
    verdwenen = BASELINE - set(bekend)
    if verdwenen:
        print(f"[orchestration] {len(verdwenen)} baseline entr(y/ies) no longer exist -- take them "
              f"out so the list keeps meaning something:", file=sys.stderr)
        for b, j in sorted(verdwenen):
            print(f"  {b}  job `{j}`", file=sys.stderr)
        return 1

    if not overtredingen:
        print("[orchestration] no ref-dispatchable workflow runs on a persistent runner")
        return 0

    print()
    print(f"[orchestration] {len(overtredingen)} job(s) on a persistent runner in a workflow "
          f"whose ref the caller chooses:")
    for bestand, job, tr in overtredingen:
        print(f"  {bestand}  job `{job}`  triggers: {tr}")
    print()
    print("[orchestration] `gh workflow run --ref <branch>` runs the workflow body from that")
    print("[orchestration] branch. A job here executes unmerged steps on the machine holding")
    print("[orchestration] the corpus, the warm cache and the runner token. Move it to")
    print("[orchestration] ubuntu-latest and let the candidate ref reach only the ephemeral")
    print("[orchestration] instance, or pin every trigger of this workflow to the default")
    print("[orchestration] branch so there is no unmerged code to carry.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
