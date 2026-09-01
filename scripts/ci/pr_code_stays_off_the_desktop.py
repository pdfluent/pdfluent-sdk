#!/usr/bin/env python3
"""A pull request must not run on the persistent runner (#311).

ci.yml states the boundary in its own header:

    xfa-fast is a persistent desktop runner, not an isolated ephemeral one, so
    a PR's Cargo build scripts/proc-macros would execute as the runner user with
    host access if PR runs used it. Only code that's already merged (push)
    touches it. This repo is private today but is slated to go public at LC10 --
    this boundary needs to hold before that, not be added after.

It was documented and not enforced, so a workflow added on 02-09-2026 broke it
on its first day -- by copying `runs-on` from a job that was already breaking it.
That is how an unenforced rule spreads: the existing code is the documentation
people actually read.

A job reachable from `pull_request` and landing on a self-hosted label runs
PR-AUTHORED files as the runner user. It does not need to be a Cargo build; every
guard job here runs `python3 scripts/ci/<something>.py` out of the checkout, and
on a pull request those scripts are whatever the PR says they are.

The rule: if a workflow can be triggered by `pull_request`, none of its jobs may
request a self-hosted runner unconditionally. Choosing per event is fine and is
what the fixed workflow does:

    runs-on: ${{ github.event_name == 'pull_request' && 'ubuntu-latest'
                 || fromJSON('["self-hosted","xfa-fast"]') }}
"""
from __future__ import annotations
import importlib.util, pathlib, sys, yaml

# ONE recogniser for the canonical runs-on expression, shared with
# orchestration_stays_hosted.py rather than written twice. Two guards reading the
# same construct with two patterns is how #1648 happened: my expression said the
# same thing the other way round, and that guard -- correctly -- did not know it.
_spec = importlib.util.spec_from_file_location(
    "orchestration_stays_hosted",
    pathlib.Path(__file__).resolve().parent / "orchestration_stays_hosted.py")
_osh = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_osh)
ALLEEN_BIJ_PUSH = _osh.ALLEEN_BIJ_PUSH

REPO = pathlib.Path(__file__).resolve().parents[2]
FLOW = REPO / ".github" / "workflows"

# Each entry is a job that predates the guard, with the reason it is still here.
# They are not exempt because they are safe -- they are the backlog this guard
# was written to close, recorded so the count cannot quietly grow. (#311)
KNOWN: dict[str, str] = {
    "ci-ephemeral.yml:create-runner": "starts the ephemeral runner PRs actually use; #311",
    "ci-ephemeral.yml:reap": "tears that runner down; must survive a cancelled run; #311",
    "ci.yml:baseline-hardware-guard": "predates this guard; #311",
    "ci.yml:orchestration-guard": "predates this guard; #311",
    "ci.yml:promise-guard": "predates this guard; #311",
    "ci.yml:measurement-guard": "predates this guard; #311",
    "ci.yml:commit-identity-guard": "predates this guard; #311",
    "security-audit.yml:cargo-audit": "predates this guard; #311",
    "security-audit.yml:cargo-deny-advisories": "predates this guard; #311",
}

MINIMUM_WORKFLOWS = 10  # FLOOR


def main() -> int:
    # GitHub reads .yaml as well. Globbing only .yml meant a workflow named
    # unsafe.yaml escaped this guard completely. (codex, #1635)
    files = sorted(list(FLOW.glob("*.yml")) + list(FLOW.glob("*.yaml")))
    if len(files) < MINIMUM_WORKFLOWS:  # FLOOR
        print(f"[pr-runner] FATAL: {len(files)} workflow(s) found, floor is "
              f"{MINIMUM_WORKFLOWS}. A scan that read almost nothing must not "
              "report a clean result.", file=sys.stderr)
        return 2

    offenders: list[str] = []
    stale: list[str] = []
    checked = 0
    seen: set[str] = set()
    for f in files:
        try:
            doc = yaml.safe_load(f.read_text()) or {}
        except yaml.YAMLError as exc:
            print(f"[pr-runner] FATAL: {f.name} does not parse: {exc}", file=sys.stderr)
            return 2
        # PyYAML reads a bare `on:` key as the boolean True.
        # `on:` has three legal shapes and PyYAML reads the bare key as True:
        #   on: pull_request            -> str
        #   on: [pull_request, push]    -> list
        #   on: {pull_request: {...}}   -> dict
        # Only the mapping was handled, so `on: [pull_request]` -- a perfectly
        # ordinary spelling -- skipped the whole workflow. (codex, #1635)
        on = doc.get("on", doc.get(True))
        if isinstance(on, str):
            events = {on}
        elif isinstance(on, list):
            events = {e for e in on if isinstance(e, str)}
        elif isinstance(on, dict):
            events = set(on)
        else:
            events = set()
        if "pull_request" not in events:
            continue
        for name, job in (doc.get("jobs") or {}).items():
            runs_on = job.get("runs-on")
            checked += 1
            if "self-hosted" not in str(runs_on):
                continue
            # Two ways to satisfy the rule, because the rule is not "no
            # self-hosted on a pull request" -- it is "no PR-AUTHORED CODE on the
            # desktop".
            #
            # First: choose the runner per event. Matched against the ONE
            # canonical form shared with orchestration_stays_hosted.py, not
            # against the substring "github.event_name" -- that accepted
            #   event_name == 'pull_request' && <self-hosted> || 'ubuntu-latest'
            # which is the rule exactly backwards, PR on the desktop. A guard
            # that approves the inverse of what it checks is worse than none.
            # (codex, #1635)
            if ALLEEN_BIJ_PUSH.search(str(runs_on)):
                continue
            # Second: stay on the desktop and pin every checkout to the BASE
            # revision, so the machine is the desktop but the code is merged
            # code. ci-ephemeral.yml's create-runner and reap need this: creating
            # and reaping instances is host work and cannot move. EVERY checkout
            # must be pinned -- one unpinned step puts the pull request's files
            # on disk, and the steps after it run from that working directory.
            checkouts = [st for st in (job.get("steps") or [])
                         if "actions/checkout" in str(st.get("uses", ""))]
            if checkouts and all(
                    "pull_request.base.sha" in str((st.get("with") or {}).get("ref", ""))
                    for st in checkouts):
                continue
            key = f"{f.name}:{name}"
            seen.add(key)
            if key not in KNOWN:
                offenders.append(
                    f"{key} runs on {runs_on} and is reachable from pull_request. "
                    "It executes PR-authored files as the runner user on the "
                    "persistent desktop.")

    # Only judge an entry whose workflow FILE is present. Absent, the register
    # cannot be evaluated at all -- and calling it stale then makes this guard
    # fail in any tree that does not contain the whole repository, which is
    # every fixture its own test builds.
    present = {f.name for f in files}
    for key in sorted(set(KNOWN) - seen):
        if key.split(":", 1)[0] not in present:
            continue
        stale.append(f"{key} is recorded as a known exception and no longer "
                     "matches anything. Remove the entry: a register that "
                     "outlives what it describes starts protecting nothing.")

    if checked == 0:
        print("[pr-runner] FATAL: no pull_request-triggered job was found at all.",
              file=sys.stderr)
        return 2

    if offenders or stale:
        print(f"[pr-runner] FAIL:", file=sys.stderr)
        for o in offenders + stale:
            print(f"    {o}", file=sys.stderr)
        # The remediation used to print the INVERSE expression -- the very form
        # finding 3 is about. Guidance that tells you to write what the guard
        # rejects is worse than no guidance: it is a wrong answer with the
        # authority of the tool. (codex, #1635)
        print("\n  Choose the runner per event, in the form both guards read:\n"
              "    runs-on: ${{ github.event_name == 'push'\n"
              "                 && fromJSON('[\"self-hosted\",\"xfa-fast\"]')\n"
              "                 || 'ubuntu-latest' }}\n"
              "\n  Or stay on the desktop and pin every checkout to the base"
              " revision:\n"
              "    ref: ${{ github.event_name == 'pull_request'\n"
              "             && github.event.pull_request.base.sha || github.sha }}",
              file=sys.stderr)
        return 1

    print(f"[pr-runner] OK: {checked} pull_request-reachable job(s); none newly "
          f"on a self-hosted runner. {len(KNOWN)} recorded from before this "
          "guard, each still to be moved (#311).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
