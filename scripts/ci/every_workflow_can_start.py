#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A key with no value is valid YAML and an invalid workflow.

On 31-08-2026 three workflows -- avrt.yml, docs-drift-guard.yml and
wasm-surface-guard.yml -- had failed every run for as long as the API went
back, and not one of them had ever succeeded. They failed like this:

    created_at  2026-08-30T08:16:57Z
    updated_at  2026-08-30T08:16:57Z
    conclusion  failure
    jobs        (none)
    log         not found

Zero seconds, zero jobs, nothing to read. The three had one thing in common and
it was one character wide. Commit d511912f deleted the only entry under their
`env:` and left the key:

    env:

    jobs:

`yaml.safe_load` reads that as `{"env": None}` and returns happily, so every
guard we own said the file was fine. GitHub parses against a schema instead, and
its schema says `env` is a mapping. It rejected the whole file -- including the
`on:` block, which is why runs appeared on pushes the file no longer asked for.

The tell is in the run's name. A workflow GitHub can read is named by its
`name:` field; one it cannot is named by its path:

    ci-ephemeral.yml     -> "CI (ephemeral Hetzner)"
    avrt.yml             -> ".github/workflows/avrt.yml"

This is the GitHub half of scripts/ci/ci_config_lint.py, which exists because
the same class of fault -- valid YAML, invalid pipeline, zero jobs, no clue --
took down .gitlab-ci.yml on 19-08-2026. Both files can be shaped wrongly while
being spelled correctly, and a parser is not a schema.

Only the null-valued key is checked here, not the whole Actions schema. It is
the fault that has actually happened, twice, and it can be judged without a
network or a linter binary on the runner. `actionlint` reports it as
`[syntax-check]` if you want a second opinion locally.

# FLOOR: workflows read >= 10 -- the repository carries around 30. A glob that
# matches nothing checks nothing, and reports it as a clean tree.
"""

from __future__ import annotations

import pathlib
import sys

import yaml

MINIMUM_WORKFLOWS = 10
FLOWS = pathlib.Path(".github/workflows")

# Keys whose value GitHub requires to be a mapping or a sequence. Present with
# no value, each one is fatal at startup: no job runs and no log is written.
#
# Deliberately a list of keys we can name rather than the whole schema. A guard
# that guesses at the schema fails good workflows, and this one runs in front of
# everybody's push.
WERKSTROOM = {"on", "jobs", "env", "defaults", "concurrency"}
BAAN = {
    "steps", "env", "strategy", "defaults", "concurrency",
    "outputs", "container", "services", "runs-on", "needs", "with", "secrets",
}
STAP = {"env", "with"}

# `on: workflow_dispatch:` and `on: push:` are null on purpose -- the event name
# is the whole statement. Nulls below `on:` are never judged, for that reason.


def leeg(waarde) -> bool:
    return waarde is None


def keur(pad: pathlib.Path) -> list[str]:
    """Every null-valued key in one workflow that GitHub would refuse."""
    try:
        doc = yaml.safe_load(pad.read_text())
    except yaml.YAMLError as fout:
        return [f"{pad.name}: not even YAML: {fout}"]
    if not isinstance(doc, dict):
        return [f"{pad.name}: top level is not a mapping"]

    fouten = []

    # `on:` is read by PyYAML as the boolean True unless it is quoted.
    for sleutel in WERKSTROOM:
        if sleutel == "on":
            aanwezig = True in doc or "on" in doc
            waarde = doc.get(True, doc.get("on"))
            if aanwezig and leeg(waarde):
                fouten.append(f"{pad.name}: top-level `on:` has no value")
            continue
        if sleutel in doc and leeg(doc[sleutel]):
            fouten.append(f"{pad.name}: top-level `{sleutel}:` has no value")

    banen = doc.get("jobs")
    if not isinstance(banen, dict):
        return fouten

    for baannaam, baan in banen.items():
        if leeg(baan):
            fouten.append(f"{pad.name}: job `{baannaam}` has no value")
            continue
        if not isinstance(baan, dict):
            continue
        for sleutel in BAAN:
            if sleutel in baan and leeg(baan[sleutel]):
                fouten.append(f"{pad.name}: {baannaam} :: `{sleutel}:` has no value")
        stappen = baan.get("steps")
        if not isinstance(stappen, list):
            continue
        for i, stap in enumerate(stappen):
            if leeg(stap):
                fouten.append(f"{pad.name}: {baannaam} :: step {i} has no value")
                continue
            if not isinstance(stap, dict):
                continue
            etiket = stap.get("name") or stap.get("uses") or f"step {i}"
            for sleutel in STAP:
                if sleutel in stap and leeg(stap[sleutel]):
                    fouten.append(
                        f"{pad.name}: {baannaam} :: {etiket} :: `{sleutel}:` has no value"
                    )
    return fouten


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[startbaar] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    fouten = []
    for pad in paden:
        fouten.extend(keur(pad))

    if len(paden) < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[startbaar] FATAL: {len(paden)} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. A glob that finds almost nothing reports a clean "
            "tree, which is exactly what an unstartable one looks like.",
            file=sys.stderr,
        )
        return 1

    if not fouten:
        print(f"[startbaar] OK: {len(paden)} workflow(s); every key GitHub needs a "
              "value for has one.")
        return 0

    print(file=sys.stderr)
    print(f"[startbaar] FATAL: {len(fouten)} key(s) present with no value. GitHub "
          "refuses the whole file: zero jobs, zero seconds, no log.", file=sys.stderr)
    for regel in fouten:
        print(f"  {regel}", file=sys.stderr)
    print(
        "\nDelete the key or give it a value. `env:` on its own is not `no "
        "environment` -- that is an absent key. A workflow in this state still "
        "produces a red run on every push, so it reads as a gate that keeps "
        "failing rather than one that has never started. (#290)",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
