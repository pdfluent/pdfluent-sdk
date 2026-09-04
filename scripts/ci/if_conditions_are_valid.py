#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`secrets` is not available in an `if:`, and using it kills the whole file.

`wasm.yml` carried

    if: |
      (startsWith(github.ref, 'refs/tags/v') || inputs.publish_npm == 'true') &&
      secrets.NPM_TOKEN != ''

The `secrets` context cannot be read from a condition. It does not evaluate to
false -- GitHub refuses the workflow file, before any job exists, with "this run
likely failed because of a workflow file issue". The WASM build had **zero
successful runs in its last sixty**, going back to 19-08-2026: nine days of a
red cross that looked like a flaky build and was a syntax error.

YAML parsers accept it, which is why nothing here noticed. The fix is to hoist
the secret into `env:` and test `env.X != ''`, which the same file already did
one step further down for a different token.

# FLOOR: workflows read >= 10 — the repository carries around 30. A scan that
# finds almost none reports a clean tree, which is what this guard exists to
# stop being mistaken for.
"""

from __future__ import annotations

import pathlib
import re
import sys

import yaml

MINIMUM_WORKFLOWS = 10
FLOWS = pathlib.Path(".github/workflows")

# Contexts GitHub refuses inside `if:`. `secrets` is the one that has bitten;
# the others are documented as unavailable there too, and cost nothing to check.
VERBODEN = re.compile(r"\b(secrets|env\.GITHUB_TOKEN|steps\.[\w-]+\.outputs\.[\w-]+\s*==\s*secrets)\.")


def condities(doc: dict):
    """Every `if:` in the file, with where it sits."""
    for jobnaam, job in (doc.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        if isinstance(job.get("if"), str):
            yield jobnaam, "<job>", job["if"]
        for stap in job.get("steps") or []:
            if isinstance(stap, dict) and isinstance(stap.get("if"), str):
                naam = stap.get("name") or stap.get("uses") or "<step>"
                yield jobnaam, str(naam)[:40], stap["if"]


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[if-conditions] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    paden = sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml"))
    treffers = []
    gelezen = 0
    for pad in paden:
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError as fout:
            print(f"[if-conditions] FATAL: {pad.name} does not parse: {fout}", file=sys.stderr)
            return 1
        if not isinstance(doc, dict):
            continue
        gelezen += 1
        for job, stap, conditie in condities(doc):
            # `secrets` in an if: is the fatal one. Reading it from `env:` is
            # fine and is the fix, so only flag the bare context.
            # Both `secrets.NPM_TOKEN` and `secrets['NPM_TOKEN']` reach the
            # same context, and GitHub refuses both. Codex made that point on
            # #1544; the first version only saw the dotted form.
            if re.search(r"(?<![\w.])secrets\s*[.\[]", conditie):
                treffers.append((pad.name, job, stap, " ".join(conditie.split())[:80]))

    if gelezen < MINIMUM_WORKFLOWS:  # FLOOR
        print(
            f"[if-conditions] FATAL: {gelezen} workflow(s) read, floor is "
            f"{MINIMUM_WORKFLOWS}. The glob found almost nothing, and then every "
            "condition looks fine.",
            file=sys.stderr,
        )
        return 1

    if treffers:
        print(
            f"[if-conditions] FATAL: {len(treffers)} condition(s) read the `secrets` "
            "context, which GitHub does not allow there:",
            file=sys.stderr,
        )
        for workflow, job, stap, conditie in treffers:
            print(f"  {workflow} :: {job} :: {stap}", file=sys.stderr)
            print(f"    {conditie}", file=sys.stderr)
        print(
            "\nThis does not evaluate to false -- it makes the whole file invalid, and "
            "the run fails before any step exists. Hoist the secret into `env:` on the "
            "job and test `env.X != ''` instead.",
            file=sys.stderr,
        )
        return 1

    print(f"[if-conditions] OK: {gelezen} workflow(s); no condition reads `secrets`.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
