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
import pathlib, sys, yaml

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
    files = sorted(FLOW.glob("*.yml"))
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
        on = doc.get("on", doc.get(True)) or {}
        if not isinstance(on, dict) or "pull_request" not in on:
            continue
        for name, job in (doc.get("jobs") or {}).items():
            runs_on = job.get("runs-on")
            checked += 1
            if "self-hosted" not in str(runs_on):
                continue
            # A per-event expression is the fix, not a violation.
            if "github.event_name" in str(runs_on):
                continue
            key = f"{f.name}:{name}"
            seen.add(key)
            if key not in KNOWN:
                offenders.append(
                    f"{key} runs on {runs_on} and is reachable from pull_request. "
                    "It executes PR-authored files as the runner user on the "
                    "persistent desktop.")

    for key in sorted(set(KNOWN) - seen):
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
        print("\n  Choose the runner per event instead:\n"
              "    runs-on: ${{ github.event_name == 'pull_request' && 'ubuntu-latest'\n"
              "                 || fromJSON('[\"self-hosted\",\"xfa-fast\"]') }}",
              file=sys.stderr)
        return 1

    print(f"[pr-runner] OK: {checked} pull_request-reachable job(s); none newly "
          f"on a self-hosted runner. {len(KNOWN)} recorded from before this "
          "guard, each still to be moved (#311).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
