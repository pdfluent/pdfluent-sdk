#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
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

# The same, for the hosted LINUX tier, which this guard counted and did not
# refuse until 05-09-2026. Empty for the same reason and on the same terms: a
# row here is an argument somebody makes in writing, not a convenience.
#
# THE PUBLIC PHASE. When this repository accepts a pull request from somebody
# other than the owner, that pull request cannot run on the persistent desktop
# -- `pr_code_stays_off_the_desktop.py` is the rule and it does not bend -- so
# hosted Linux comes back for exactly those jobs, and rows land here with that
# reason. What must not come back is what #333 removed: the same guard billed
# twice for one change, once on the pull request and once on the push behind it.
LINUX_TOEGESTAAN: dict[tuple[str, str], str] = {}


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
            # `tags:` alone is a release. `tags:` with `branches:` fires on
            # every commit too -- and so does `tags:` with `branches-ignore:`,
            # which enables every branch the filter does not exclude. Codex
            # caught the second form on #1546; the first version only knew
            # about `branches`.
            takken = "branches" in waarde or "branches-ignore" in waarde
            if "tags" in waarde and not takken:
                continue
        return True
    return False


def runner_ingangen(job: dict):
    """Every runner this job could land on, one entry at a time.

    A blob of text will not do. A matrix carrying both `[self-hosted, xfa-fast]`
    and `macos-latest` contains the word "self-hosted", and deciding on the blob
    excuses exactly the expensive half; deciding the other way bills our own
    Windows box, whose label contains "windows". Codex made both points on
    #1546. So: collect the entries and judge each on its own.
    """
    uit = []

    def voeg_toe(waarde):
        if isinstance(waarde, str):
            uit.append(waarde)
        elif isinstance(waarde, list):
            # A list is either one runner's labels, or a matrix of runners.
            if all(isinstance(x, str) for x in waarde):
                uit.append(",".join(waarde))
            else:
                for x in waarde:
                    voeg_toe(x)
        elif isinstance(waarde, dict):
            for x in waarde.values():
                voeg_toe(x)

    runs_on = job.get("runs-on")
    if isinstance(runs_on, str) and "${{" in runs_on:
        # Resolved from the matrix; the matrix values are the real answer.
        matrix = (job.get("strategy") or {}).get("matrix") if isinstance(job.get("strategy"), dict) else None
        if matrix:
            voeg_toe(matrix)
        else:
            uit.append(runs_on)
    else:
        voeg_toe(runs_on)
    return [x for x in uit if isinstance(x, str) and x]


def kosten(ingang: str) -> tuple[str, int] | None:
    """What one runner entry costs. Our own machines cost nothing."""
    if "self-hosted" in ingang:
        return None
    for merk, factor in DUUR.items():
        if merk in ingang:
            return merk, factor
    return None


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[hosted-minutes] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    gelezen = 0
    duur, goedkoop = [], []

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
            ingangen = runner_ingangen(job)
            duurste = None
            for ingang in ingangen:
                gevonden = kosten(ingang)
                if gevonden and (duurste is None or gevonden[1] > duurste[1]):
                    duurste = gevonden
            if duurste is None:
                if any("ubuntu" in i and "self-hosted" not in i for i in ingangen):
                    if (pad.name, naam) not in LINUX_TOEGESTAAN:
                        goedkoop.append((pad.name, naam))
                continue
            if (pad.name, naam) in TOEGESTAAN:
                continue
            duur.append((pad.name, naam, duurste[0], duurste[1]))

    if gelezen < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[hosted-minutes] FATAL: {gelezen} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. A glob that finds almost nothing reports a clean "
            "tree, which is what an unnoticed bill looks like.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[hosted-minutes] {gelezen} workflow(s); {len(goedkoop)} ubuntu job(s) on "
        f"automatic triggers, {len(duur)} expensive one(s)"
    )

    if goedkoop:
        print(file=sys.stderr)
        print(f"[hosted-minutes] FATAL: {len(goedkoop)} job(s) use a hosted Linux "
              "runner on a trigger that fires by itself:", file=sys.stderr)
        for workflow, job in sorted(goedkoop):
            print(f"  {workflow} :: {job}", file=sys.stderr)
        print(
            "\nubuntu-latest used to be counted here and not refused, on the "
            "reasoning that Linux is the cheap tier and some of it is seconds of "
            "guard work. Measured 05-09-2026: the 2,000 included minutes were "
            "gone and the account had begun billing an Actions budget, and the "
            "cause was the cheap tier -- about 180 jobs in fourteen hours, each "
            "billed as at least a whole minute, most of them a guard running "
            "twice for the same change. Cheap per minute is not cheap per "
            "hundred-and-eighty. (#333)\n\n"
            "  Move it to [self-hosted, xfa-fast] if it does not compile, or to "
            "the ephemeral instance in ci-ephemeral.yml if it does. A tag is a "
            "release and a dispatch is a decision; both may still cost.",
            file=sys.stderr,
        )
        return 1

    if not duur:
        print("[hosted-minutes] nothing that fires by itself reaches a hosted runner "
              "at all, on any tier")
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
