#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that calls a reusable workflow grants every scope that workflow asks
for.

GitHub refuses the WHOLE workflow when it does not -- before a job starts, with
no log to read and `startup_failure` as the only word for it. Nothing on the
caller's side changed; a scope was added on the callee's side and the caller
fell behind.

Measured 09-09-2026: `security-audit.yml` grew `checks: write` (the audit action
posts its findings as a check-run and cannot without it). `ci-ephemeral.yml`
calls that workflow and grants `issues` and `pull-requests` only, so every wave
from that landing on -- the two-hourly schedule and every dispatch -- was
refused. The message is only visible to somebody who asks the API for it:

    Invalid workflow file: ci-ephemeral.yml#L684. Error calling workflow
    security-audit.yml. The workflow is requesting 'checks: write', but is
    only allowed 'checks: none'.

The same shape cost run 34050078405 when #350 landed, which is why the caller
has a permissions block at all. Twice is a rule.

Only `write` requests are compared, and only `permissions:` blocks written as a
mapping. Two deliberate limits:

  * A caller that names no permissions holds this repository's default token,
    which is read-only -- so a `read` request is already satisfied and a
    `write` one never is. Checking `read` would flag the four correct calls in
    `ci-ephemeral.yml` that rely on exactly that.
  * `write-all`, `read-all` and the string forms are left alone. They are not
    lists of scopes, and a guard that guessed at them would fail on files that
    are correct.
"""
from __future__ import annotations

import pathlib
import sys

import yaml

WORTEL = pathlib.Path(__file__).resolve().parents[2]
WORKFLOWS = WORTEL / ".github" / "workflows"

# A callee asking for `read` is satisfied by a caller holding `write`.
RANG = {"none": 0, "read": 1, "write": 2}


def scopes(blok) -> dict[str, str] | None:
    """The scope mapping of a `permissions:` block, or None if it is not one."""
    if not isinstance(blok, dict):
        return None
    return {str(k): str(v) for k, v in blok.items()}


def gevraagd(pad: pathlib.Path) -> dict[str, str]:
    """What a called workflow asks for: its workflow-level block, plus the
    strongest scope any of its jobs names.

    A job-level block is part of the request too. GitHub compares the caller's
    grant against everything the callee can use, not only the file header.
    """
    doc = yaml.safe_load(pad.read_text(encoding="utf-8")) or {}
    uit: dict[str, str] = {}

    def voeg_toe(blok) -> None:
        for scope, waarde in (scopes(blok) or {}).items():
            if RANG.get(waarde, 0) > RANG.get(uit.get(scope, "none"), 0):
                uit[scope] = waarde

    voeg_toe(doc.get("permissions"))
    for job in (doc.get("jobs") or {}).values():
        if isinstance(job, dict):
            voeg_toe(job.get("permissions"))
    return {s: w for s, w in uit.items() if w == "write"}


def main() -> int:
    if not WORKFLOWS.is_dir():
        print("[caller-grants] FATAL: no .github/workflows directory",
              file=sys.stderr)
        return 2

    problemen: list[str] = []
    aanroepen = 0

    for pad in sorted(WORKFLOWS.glob("*.yml")):
        try:
            doc = yaml.safe_load(pad.read_text(encoding="utf-8")) or {}
        except yaml.YAMLError as fout:
            print(f"[caller-grants] FATAL: {pad.name} is not YAML: {fout}",
                  file=sys.stderr)
            return 2
        if not isinstance(doc, dict):
            continue
        bestands_grant = scopes(doc.get("permissions")) or {}

        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            roept = job.get("uses")
            # Only calls to a workflow in this repository: a call to another
            # repository's workflow cannot be read here, and guessing at it
            # would report on a file this check cannot see.
            if not isinstance(roept, str) or not roept.startswith("./"):
                continue
            doel = WORTEL / roept.split("@")[0].removeprefix("./")
            if not doel.is_file():
                problemen.append(f"{pad.name}:{naam} calls {roept}, which does "
                                 "not exist")
                continue
            aanroepen += 1

            grant = scopes(job.get("permissions"))
            if grant is None:
                grant = bestands_grant
            for scope, waarde in sorted(gevraagd(doel).items()):
                heeft = grant.get(scope, "none")
                if RANG.get(heeft, 0) >= RANG.get(waarde, 0):
                    continue
                problemen.append(
                    f"{pad.name}:{naam} calls {doel.name}, which asks for "
                    f"`{scope}: {waarde}`, and grants `{scope}: {heeft}`. "
                    "GitHub refuses the whole workflow before any job starts, "
                    "with startup_failure and no log.")

    if aanroepen == 0:
        print("[caller-grants] FATAL: no call to a workflow in this repository "
              "was found. A scan that looked at nothing cannot report "
              "agreement.", file=sys.stderr)
        return 2
    if problemen:
        print(f"[caller-grants] FAIL: {len(problemen)} call(s) grant too little:",
              file=sys.stderr)
        for p in sorted(set(problemen)):
            print(f"    {p}", file=sys.stderr)
        return 1
    print(f"[caller-grants] OK: {aanroepen} call(s); each grants what it calls")
    return 0


if __name__ == "__main__":
    sys.exit(main())
