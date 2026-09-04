#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A job that tests `github.event.schedule` must name a cron the workflow has.

`fuzz.yml` declares two crons and its jobs pick between them with
`if: github.event.schedule == '...'`. Moving the crons to stop them colliding
with the nightly suite left those conditions pointing at the old times, so
`smoke` and `deep` matched nothing and silently stopped running (#274).

Nothing failed. A job whose `if` is false is skipped, and a skipped job is
green -- so the fuzzing quietly went away while the workflow kept reporting
success. Codex caught it on #1546.

# NO-FLOOR: it compares two lists inside each file. There is nothing it can
# find less of without the file itself being gone.
"""

from __future__ import annotations

import pathlib
import re
import sys

import yaml

FLOWS = pathlib.Path(".github/workflows")
SCHEDULE_IF = re.compile(r"github\.event\.schedule\s*==\s*'([^']+)'")


def main() -> int:
    if not FLOWS.is_dir():
        print(f"[cron-guards] FATAL: {FLOWS} is missing", file=sys.stderr)
        return 1

    treffers, bekeken = [], 0
    for pad in sorted(FLOWS.glob("*.yml")) + sorted(FLOWS.glob("*.yaml")):
        try:
            doc = yaml.safe_load(pad.read_text()) or {}
        except yaml.YAMLError:
            continue
        if not isinstance(doc, dict):
            continue
        on = doc.get(True) or doc.get("on") or {}
        crons = set()
        if isinstance(on, dict):
            for item in on.get("schedule") or []:
                if isinstance(item, dict) and item.get("cron"):
                    crons.add(item["cron"].strip())
        for naam, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            for m in SCHEDULE_IF.finditer(str(job.get("if", ""))):
                bekeken += 1
                if m.group(1).strip() not in crons:
                    treffers.append((pad.name, naam, m.group(1), sorted(crons)))

    print(f"[cron-guards] {bekeken} schedule condition(s) checked")
    if not treffers:
        print("[cron-guards] OK: each names a cron its workflow declares.")
        return 0

    print(file=sys.stderr)
    print(f"[cron-guards] FATAL: {len(treffers)} condition(s) name a cron that is not "
          "declared:", file=sys.stderr)
    for workflow, job, wilde, heeft in treffers:
        print(f"  {workflow} :: {job} wants '{wilde}'; the workflow declares {heeft}",
              file=sys.stderr)
    print(
        "\nA job whose `if` is false is skipped, and a skipped job is green. So this "
        "does not fail -- it makes the work quietly stop while the workflow keeps "
        "reporting success.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
