#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""One event must not start more than one machine.

On 28-08-2026 Hetzner sent five "server created" mails inside five minutes for
a single push. Nothing had leaked and nothing was stuck: five workflows had
each been given their own `create-runner`, they all fire on the same event, and
each one dutifully made a cpx42.

Cheap per workflow, five times the cost per push. The mistake is easy to make
again, because each file looks correct on its own -- which is exactly why it
belongs in a guard rather than in someone's memory (#275).

A `schedule` counts per cron expression: two workflows on the same cron start
together just as surely as two on `push`.

# FLOOR: workflows read >= 10 — the repository carries around 30. A glob that
# finds nothing sees no collisions either, which reads like a clean tree.
"""

from __future__ import annotations

import collections
import pathlib
import sys

import yaml

MINIMUM_WORKFLOWS = 10
FLOWS = pathlib.Path(".github/workflows")
AUTOMATISCH = ("push", "pull_request", "pull_request_target", "workflow_run")


def gebeurtenissen(on) -> list[str]:
    """The events that can start this workflow without anyone asking."""
    if isinstance(on, str):
        return [on] if on in AUTOMATISCH else []
    if isinstance(on, list):
        return [t for t in on if t in AUTOMATISCH]
    if not isinstance(on, dict):
        return []
    uit = []
    for naam, waarde in on.items():
        if naam in AUTOMATISCH:
            # `push` with only a tags filter is a release, and releases are rare
            # enough that two of them overlapping is not the problem here.
            if naam == "push" and isinstance(waarde, dict):
                if "tags" in waarde and "branches" not in waarde and "branches-ignore" not in waarde:
                    continue
            uit.append(naam)
        elif naam == "schedule":
            for item in waarde or []:
                if isinstance(item, dict) and item.get("cron"):
                    uit.append(f"schedule:{item['cron']}")
    return uit


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[one-instance] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    gelezen = 0
    per_gebeurtenis: dict[str, list[str]] = collections.defaultdict(list)

    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as fout:
            print(f"[one-instance] FATAL: {pad.name} does not parse: {fout}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict):
            continue
        gelezen += 1
        jobs = doc.get("jobs") or {}
        if not any(naam == "create-runner" or "hcloud-github-runner" in str(job)
                   for naam, job in jobs.items()):
            continue
        for gebeurtenis in gebeurtenissen(doc.get(True) or doc.get("on")):
            per_gebeurtenis[gebeurtenis].append(pad.name)

    if gelezen < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[one-instance] FATAL: {gelezen} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. A glob that finds nothing sees no collisions "
            "either, which reads like a clean tree.",
            file=sys.stderr,
        )
        return 1

    botsingen = {g: w for g, w in per_gebeurtenis.items() if len(w) > 1}
    print(
        f"[one-instance] {gelezen} workflow(s); "
        f"{len(per_gebeurtenis)} automatic event(s) start a machine"
    )

    if not botsingen:
        print("[one-instance] no event starts more than one")
        return 0

    print(file=sys.stderr)
    print(f"[one-instance] FATAL: {len(botsingen)} event(s) start more than one machine:",
          file=sys.stderr)
    for gebeurtenis, workflows in sorted(botsingen.items()):
        print(f"  {gebeurtenis}: {', '.join(sorted(workflows))}", file=sys.stderr)
    print(
        "\nEach of those files is correct on its own, which is why this is a guard and "
        "not a note. They fire together, so a single push pays for one instance per "
        "workflow -- five cpx42s inside five minutes, on 28-08-2026. Put the work on "
        "the instance one of them already creates, or move it off the shared trigger. "
        "(#275)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
