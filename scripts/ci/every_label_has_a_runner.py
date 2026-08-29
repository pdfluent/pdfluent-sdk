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
        # Every trigger, not only the automatic ones. Three runs sat queued for
        # nine hours on a label no runner carried (#281) and this guard reported
        # OK, because their workflows are dispatch-only and fell outside exactly
        # the check written to catch them. A job that can never be assigned is
        # stuck whoever started it.
        vanzelf = namen or ["(no trigger)"]
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
        print("[labels] OK: every self-hosted label any job asks for has a runner.")
        return 0

    # Known and tracked in #276: four dispatch-only jobs want a corpus runner
    # that was never provisioned. Named individually so the list cannot quietly
    # grow, and checked in both directions so it cannot quietly shrink either --
    # a baseline that only fails upward stops being a baseline.
    BEKEND = {
        ("bench.yml", "benchmark", "xfa-corpus"),
        ("crash-guard.yml", "crash-guard", "xfa-corpus"),
        ("gate-ci.yml", "gate", "xfa-corpus"),
        ("wasm-gate.yml", "wasm-gate", "xfa-corpus"),
    }
    # The exemption is for jobs nobody can start by accident. If one of these
    # workflows re-enables push or schedule it queues on every commit, which is
    # the failure this guard exists for -- so the trigger list is part of what
    # is excused, not something the baseline may drop.
    HANDMATIG = {"workflow_dispatch", "workflow_call", "repository_dispatch"}
    handmatig_stuk = {(w, j, l) for w, j, l, t in ontbreekt if set(t or []) <= HANDMATIG}
    automatisch = [r for r in ontbreekt if not set(r[3] or []) <= HANDMATIG]

    def uitleg(rijen) -> None:
        for workflow, job, label, triggers in rijen:
            print(f"  {workflow} :: {job} wants `{label}` on {triggers}", file=sys.stderr)
        print(
            "\nThose runs queue until GitHub abandons them about a day later. A queued "
            "job is not red -- it waits, and a gate that waits forever looks like one "
            "that has not got round to you yet. Register the runner, point the job at "
            "one that exists, or make the workflow dispatch-only until it can run. "
            "(#276)",
            file=sys.stderr,
        )

    if automatisch:
        print(file=sys.stderr)
        print(f"[labels] FATAL: {len(automatisch)} job(s) ask for a missing label on an "
              "automatic trigger; no baseline covers those.", file=sys.stderr)
        uitleg(automatisch)
        return 1

    nieuw = handmatig_stuk - BEKEND
    if nieuw:
        print(file=sys.stderr)
        print(f"[labels] FATAL: {len(nieuw)} job(s) ask for a label no runner answers:",
              file=sys.stderr)
        uitleg([r for r in ontbreekt if (r[0], r[1], r[2]) in nieuw])
        return 1

    opgelost = BEKEND - handmatig_stuk
    if opgelost:
        print(f"FAIL: {len(opgelost)} known-stuck job(s) can now be assigned: "
              f"{sorted(opgelost)}. Remove them from BEKEND so the next one is caught.",
              file=sys.stderr)
        return 1

    print(f"[labels] OK: {len(BEKEND)} job(s) still wait on a corpus runner (#276); "
          "no new label is unanswered.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
