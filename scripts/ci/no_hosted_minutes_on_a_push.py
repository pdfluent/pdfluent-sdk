#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""No macOS or Windows runner on a trigger that fires by itself.

GitHub bills macOS at ten times the Linux rate and Windows at twice. On
28-08-2026 the Actions budget stood at 90% used, and the reason was one line:

    node-bindings.yml
    on:
      push:
        tags: ['node/v*', 'v*.*.*']
        branches: [master]        <-- this

`branches: [master]` alongside the tags meant every ordinary push to master ran
`build-and-test` across ubuntu, macOS and Windows, and `build-release` across
macos-13 and macos-latest. A matrix built to answer "do the bindings still build
for a release?" was answering it on every commit, in the most expensive place
there is.

A tag is a release and a dispatch is a decision. Both may cost. A push to a
branch, a pull request and a schedule happen on their own, and nothing that
happens on its own should reach for a ten-times runner.

Linux is a different question -- ubuntu-latest is the cheap tier and some of it
is seconds of guard work -- so this guard counts it and prints the total rather
than refusing it. Moving that to ephemeral instances is #267.

# FLOOR: workflows read >= 10 — the repository carries around 30. A glob that
# finds almost none reports a clean tree, which is exactly what an unnoticed
# bill looks like.
"""

from __future__ import annotations

import pathlib
import sys

import yaml

MINIMUM_WORKFLOWS = 10
FLOWS = pathlib.Path(".github/workflows")

# Triggers that fire without anyone deciding to spend anything.
VANZELF = {"push", "pull_request", "pull_request_target", "schedule", "workflow_run"}

DUUR = {"macos": 10, "windows": 2}

# Jobs allowed an expensive runner on an automatic trigger, with the reason.
# Empty on purpose: there is no such case today, and adding one should be an
# argument someone makes in writing.
TOEGESTAAN: dict[tuple[str, str], str] = {}


def vanzelf_actief(on) -> bool:
    """True when this workflow can start without a person choosing to."""
    if isinstance(on, str):
        return on in VANZELF
    if isinstance(on, list):
        return any(t in VANZELF for t in on)
    if not isinstance(on, dict):
        return False
    for naam, waarde in on.items():
        if naam not in VANZELF:
            continue
        # `push` with only a `tags:` filter is a release, not an ordinary push.
        if naam == "push" and isinstance(waarde, dict):
            if "tags" in waarde and "branches" not in waarde:
                continue
        return True
    return False


def kosten(tekst: str) -> tuple[str, int] | None:
    for merk, factor in DUUR.items():
        if merk in tekst:
            return merk, factor
    return None


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[hosted-minutes] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    gelezen = 0
    duur, goedkoop = [], 0

    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as fout:
            print(f"[hosted-minutes] FATAL: {pad.name} does not parse: {fout}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict):
            continue
        gelezen += 1
        if not vanzelf_actief(doc.get(True) or doc.get("on")):
            continue
        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            tekst = str(job.get("runs-on", "")) + str(job.get("strategy", ""))
            if "self-hosted" in tekst:
                continue
            gevonden = kosten(tekst)
            if gevonden is None:
                if "ubuntu" in tekst:
                    goedkoop += 1
                continue
            if (pad.name, naam) in TOEGESTAAN:
                continue
            duur.append((pad.name, naam, gevonden[0], gevonden[1]))

    if gelezen < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[hosted-minutes] FATAL: {gelezen} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. A glob that finds almost nothing reports a clean "
            "tree, which is what an unnoticed bill looks like.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[hosted-minutes] {gelezen} workflow(s); {goedkoop} ubuntu job(s) on automatic "
        f"triggers, {len(duur)} expensive one(s)"
    )

    if not duur:
        print("[hosted-minutes] nothing that fires by itself reaches for macOS or Windows")
        return 0

    print(file=sys.stderr)
    print(f"[hosted-minutes] FATAL: {len(duur)} job(s) use a costed runner on a trigger "
          "that fires by itself:", file=sys.stderr)
    for workflow, job, merk, factor in sorted(duur):
        print(f"  {workflow} :: {job}  ({merk}, {factor}x the Linux rate)", file=sys.stderr)
    print(
        "\nA tag is a release and a dispatch is a decision; both may cost. A push to a "
        "branch, a pull request and a schedule happen on their own. Restrict the "
        "trigger to tags, or move the job to a self-hosted or ephemeral runner. If it "
        "genuinely has to be here, put it in TOEGESTAAN with the argument.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
