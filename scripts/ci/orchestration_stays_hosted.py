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
WORTEL = REPO
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


# A job is heavy when it compiles the workspace. Those belong on a throwaway
# instance: the persistent desktop has four cores and also carries the corpus,
# the warm cargo cache and the runner registration token. On 28-08-2026 two CI
# systems were compiling on it at once -- load 5.6 on four cores, neither of them
# orphaned. The saturation was the design, not a fault (#267).
ZWAAR = re.compile(r"\bcargo\s+(build|test|check|clippy|bench|doc)\b")

# Jobs that compile on a persistent runner because they read the corpus, which
# lives there. Moving them would mean shipping the corpus to a throwaway
# instance on every run. Recorded so the count cannot grow, and so that removing
# one is a decision rather than a line that ages.
ZWARE_BASELINE = {
    ("bench.yml", "benchmark"),
    ("crash-guard.yml", "crash-guard"),
    ("gate-ci.yml", "gate"),
    ("wasm-gate.yml", "wasm-gate"),
    # Only the build job compiles; the other two consume its artefact.
    ("enterprise-acceptance.yml", "build"),
    # Not corpus, but release. `preflight` runs a full workspace check and the
    # integration suites before a publish, so it belongs on a throwaway instance
    # like everything else here. It stays for now because it is on the release
    # path and there is no way to rehearse a change to it without publishing:
    # a broken preflight surfaces at the worst possible moment. Moving it is
    # tracked separately on #267 rather than done blind.
    ("publish-crates.yml", "preflight"),
    # Found only once the guard started following the scripts a job calls:
    # `publish` runs ./scripts/publish_ordered.sh, which compiles. Same reason
    # as preflight -- release path, no way to rehearse a change to it without
    # publishing. (Codex, #1541)
    ("publish-crates.yml", "publish"),
}


# A job that calls `bash scripts/ci/run_build.sh` compiles just as hard as one
# that types `cargo build`, and the workflow file says nothing about it. Codex
# raised this on #1541; the first version read only the YAML.
# Only shell scripts. A .sh file's text is commands, so `cargo build` in it is a
# build. A .py file's text is mostly not: this guard's own source contains
# `cargo build|test|check` inside the pattern below, and following it made the
# job that runs this guard look like a compile.
AANGEROEPEN = re.compile(r"(?:bash |sh |\./)?(scripts/[\w/.-]+\.sh)")


def compileert(job, wortel: pathlib.Path) -> bool:
    """True when this job builds -- directly, or through a script it calls."""
    if not isinstance(job, dict):
        return False
    for stap in job.get("steps") or []:
        if not (isinstance(stap, dict) and isinstance(stap.get("run"), str)):
            continue
        script = stap["run"]
        if ZWAAR.search(script):
            return True
        for m in AANGEROEPEN.finditer(script):
            pad = wortel / m.group(1)
            if not pad.is_file():
                continue
            try:
                if ZWAAR.search(pad.read_text(errors="replace")):
                    return True
            except OSError:
                continue
    return False


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
# BESLUIT 28-08-2026 (Jasper): "Ik zou op dit moment niet te bevreesd daarover
# zijn zolang ik de enige contributor ben. Hou kosten laag maar zorg dat
# pipelines wel snel gaan en development niet tegenhoudt."
#
# That settles what this file could not settle by itself. The exposure is real
# -- a pull_request runs the merge commit, so a branch can put its own steps on
# the persistent desktop -- and on a private repository with one contributor
# there is no second party to protect against. Fork pull requests would change
# that, and there are none.
#
# So these stay recorded rather than fixed, and the guard's sharp end moves to
# ZWARE_BASELINE: heavy work must not run on that machine, because four cores
# shared with the corpus is a speed problem no matter who wrote the branch.
#
# If a second contributor appears, this is the first decision to revisit
# (#266, #268).
#
# ci-ephemeral.yml orchestrates from the desktop for pull requests too, under
# that same decision: the heavy build goes to a throwaway instance, which is
# both free and faster than a hosted runner.
BASELINE = {
    # The light guard work, now one job. It moved off ubuntu-latest on 28-08-2026 when the
    # Actions budget ran out and every hosted job started failing outright --
    # a spending limit disables hosted runners and leaves self-hosted ones
    # working, so this is where the guards keep running at all. Seconds of file
    # scanning each; booting an instance would cost more than the work. Under
    # the same 28-08 decision about branch code on the desktop (#274).
    ("ci.yml", "orchestration-guard"),
    # #288 moved thirteen guards off the GitLab mirror, where a failure stopped
    # nothing, into these two jobs. They land on the desktop for the same reason
    # orchestration-guard did: a spending limit disables hosted runners and
    # leaves self-hosted ones working, and a guard that stops on the day the
    # bill stops is worthless on the day it matters. Python file scans plus
    # `cargo metadata` and `cargo tree` -- no compilation, seconds of work.
    ("ci.yml", "promise-guard"),
    ("ci.yml", "measurement-guard"),
    ("security-audit.yml", "cargo-audit"),
    ("security-audit.yml", "cargo-deny-advisories"),
    # Orchestration for a pull request, under the 28-08 decision above: the
    # heavy build goes to a throwaway instance and the desktop only creates and
    # deletes it. A PR branch could change what those two jobs do; accepted
    # while this repository has one contributor. Only ci-ephemeral does this
    # now -- the others hand their work to the instance it creates (#275).
    ("ci-ephemeral.yml", "create-runner"),
    # `reap` is the other half of `create-runner`: the same workflow deletes the
    # instance it made. It was left out when create-runner was written down, and
    # this check has been failing on it on master ever since -- which is what
    # #288 is about in miniature, since the failure was in a step nothing read.
    ("ci-ephemeral.yml", "reap"),
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
    zware, zwaar_bekend = [], []
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        try:
            doc = yaml.load(pad.read_text(), GeenDubbeleSleutels) or {}
        except yaml.YAMLError as e:
            print(f"[orchestration] FATAL: {pad.name} will not parse: {e}", file=sys.stderr)
            return 1
        bekeken += 1
        tr = triggers(doc)

        # The heavy-job rule does not care which trigger fired: compiling the
        # workspace on the persistent desktop saturates it whether the push was
        # reviewed or not.
        for naam, job in (doc.get("jobs") or {}).items():
            if not op_blijvende_runner(job.get("runs-on", "")) or not compileert(job, WORTEL):
                continue
            if (pad.name, naam) in ZWARE_BASELINE:
                zwaar_bekend.append((pad.name, naam))
            else:
                zware.append((pad.name, naam))

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

    print(f"[orchestration] {bekeken} workflow(s) inspected, {len(bekend)} known exposure(s), "
          f"{len(zwaar_bekend)} known heavy job(s) on the desktop")

    zwaar_verdwenen = [k for k in ZWARE_BASELINE if k not in set(zwaar_bekend)]
    if zwaar_verdwenen:
        print(f"[orchestration] {len(zwaar_verdwenen)} entr(y/ies) in ZWARE_BASELINE no longer "
              "match a compiling job on a persistent runner. Take them out, or the list stops "
              "describing anything:", file=sys.stderr)
        for w, j in sorted(zwaar_verdwenen):
            print(f"  {w}  job `{j}`", file=sys.stderr)
        return 1

    if zware:
        print(file=sys.stderr)
        print(f"[orchestration] {len(zware)} job(s) compile the workspace on the persistent "
              "runner:", file=sys.stderr)
        for w, j in sorted(zware):
            print(f"  {w}  job `{j}`", file=sys.stderr)
        print(file=sys.stderr)
        print("[orchestration] That machine has four cores and also carries the corpus, the",
              file=sys.stderr)
        print("[orchestration] warm cargo cache and the runner registration token. A cargo",
              file=sys.stderr)
        print("[orchestration] build belongs on a throwaway instance -- see",
              file=sys.stderr)
        print("[orchestration] .github/workflows/ci-ephemeral.yml for the shape. If the job",
              file=sys.stderr)
        print("[orchestration] genuinely needs the corpus, add it to ZWARE_BASELINE with the",
              file=sys.stderr)
        print("[orchestration] reason. (#267)", file=sys.stderr)
        return 1


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
