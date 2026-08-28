#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A self-hosted label nobody answers is a queue entry pretending to be a gate.

Four workflows asked for `[self-hosted, xfa-corpus]`. That runner does not
exist: this repository has one self-hosted runner, labelled `xfa-fast`. So every
automatic run of those four sat in the queue until GitHub abandoned it roughly a
day later -- holding a slot, keeping the pipeline permanently busy, and testing
nothing (#276).

It never showed as a failure. A queued job is not red; it simply waits, and a
gate that waits forever looks exactly like a gate that has not got round to you
yet.

Labels that resolve to a machine we rent on demand are exempt: an ephemeral
runner's label is created at the moment the instance registers, so it is
correctly absent when nothing is running.

# NO-FLOOR: this compares two lists that both come from live sources. It cannot
# quietly find fewer -- an unreachable source is announced instead.
"""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys

import yaml

FLOWS = pathlib.Path(".github/workflows")
# Created when an instance registers itself, so absent by design between runs.
EFEMEER = re.compile(r"needs\.|matrix\.|^hetzner$|^gh-runner-")


def geregistreerd() -> set[str] | None:
    # `gh` is not installed on the desktop runner, and OSError from subprocess is
    # not a return code -- an uncaught one exits before a single line is printed,
    # which is a crash pretending to be a failed check. Missing tooling has to
    # announce itself.
    try:
        r = subprocess.run(
            ["gh", "api", "repos/{owner}/{repo}/actions/runners", "--jq",
             "[.runners[] | select(.status==\"online\") | .labels[].name] | unique"],
            capture_output=True, text=True, check=False, timeout=60,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if r.returncode != 0:
        return None
    try:
        return set(json.loads(r.stdout))
    except json.JSONDecodeError:
        return None


def gevraagd(job) -> list[str]:
    ro = job.get("runs-on")
    if isinstance(ro, str):
        return [] if "${{" in ro else [ro]
    if isinstance(ro, list):
        return [x for x in ro if isinstance(x, str)]
    return []


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[labels] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    online = geregistreerd()
    if online is None:
        print("SKIPPED (not a pass): could not read the runner list, so no label was "
              "checked against anything.", file=sys.stderr)
        return 0

    ontbreekt = []
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError:
            continue
        if not isinstance(doc, dict):
            continue
        on = doc.get(True) or doc.get("on") or {}
        namen = list(on) if isinstance(on, dict) else [on]
        vanzelf = [t for t in namen if t in ("push", "pull_request", "schedule", "workflow_run")]
        if not vanzelf:
            continue
        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            labels = gevraagd(job)
            if "self-hosted" not in labels:
                continue
            for label in labels:
                if label == "self-hosted" or EFEMEER.search(label):
                    continue
                if label not in online:
                    ontbreekt.append((pad.name, naam, label, vanzelf))

    print(f"[labels] online labels: {', '.join(sorted(online)) or 'none'}")
    if not ontbreekt:
        print("[labels] OK: every self-hosted label an automatic job asks for has a runner.")
        return 0

    print(file=sys.stderr)
    print(f"[labels] FATAL: {len(ontbreekt)} job(s) ask for a label no runner answers:",
          file=sys.stderr)
    for workflow, job, label, triggers in ontbreekt:
        print(f"  {workflow} :: {job} wants `{label}` on {triggers}", file=sys.stderr)
    print(
        "\nThose runs queue until GitHub abandons them about a day later. A queued job "
        "is not red -- it waits, and a gate that waits forever looks like one that has "
        "not got round to you yet. Register the runner, point the job at one that "
        "exists, or make the workflow dispatch-only until it can run. (#276)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
