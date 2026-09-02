#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""The range selection of the internal-matter guard answers, or refuses.

`internal_matter_range.sh` decides which commits `geen_interne_zaken.py`
reads. It was inline in the workflow until 02-09-2026, when it turned out to
have measured an empty range on every master push since the day the job
landed: `origin/master..HEAD` on a checkout where HEAD *is* origin/master. The
guard refused that range, correctly, and the job was red for hours before
anyone read why. A selection that lives in a shell `case` inside YAML has no
test, so this file exists and the selection moved out.

Every case below asserts the exact range string or the exact refusal. An
approximate check ("exit code is non-zero") would have accepted a `set -u`
abort as a refusal; the refusal has to be the one with the wording, on
stderr, because that wording is what a reader of the job log gets.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "internal_matter_range.sh"
ZEROS = "0" * 40
PARENT = "a" * 40

# The count the loop below has to reach. A case that silently never runs is
# indistinguishable from a passing one; the floor makes that visible.
MIN_CASES = 8


def run(env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    clean = {k: v for k, v in os.environ.items() if k not in {"EVENT", "PUSH_BEFORE"}}
    clean.update(env)
    return subprocess.run(
        ["sh", str(SCRIPT)], env=clean, capture_output=True, text=True, check=False
    )


def main() -> int:
    fouten: list[str] = []
    ran = 0

    def expect(name: str, ok: bool, detail: str) -> None:
        nonlocal ran
        ran += 1
        print(f"  {'ok  ' if ok else 'FAIL'} {name}")
        if not ok:
            fouten.append(f"{name}: {detail}")

    if not SCRIPT.is_file():
        print(f"SKIPPED (not a pass): {SCRIPT} is missing", file=sys.stderr)
        return 1

    # --- the two events the workflow has -----------------------------------
    r = run({"EVENT": "pull_request"})
    expect("a pull request reads the branch against origin/master",
           r.returncode == 0 and r.stdout == "origin/master..HEAD\n",
           f"rc={r.returncode} out={r.stdout!r} err={r.stderr!r}")

    r = run({"EVENT": "pull_request", "PUSH_BEFORE": PARENT})
    expect("a stray PUSH_BEFORE does not change a pull request's range",
           r.returncode == 0 and r.stdout == "origin/master..HEAD\n",
           f"rc={r.returncode} out={r.stdout!r}")

    r = run({"EVENT": "push", "PUSH_BEFORE": PARENT})
    expect("a push reads exactly what it added",
           r.returncode == 0 and r.stdout == f"{PARENT}..HEAD\n",
           f"rc={r.returncode} out={r.stdout!r} err={r.stderr!r}")

    # --- a push that cannot say what it added is not a pass ------------------
    for label, env in (
        ("a first push (all-zero before) is not a pass", {"EVENT": "push", "PUSH_BEFORE": ZEROS}),
        ("a push with an empty before is not a pass", {"EVENT": "push", "PUSH_BEFORE": ""}),
        ("a push with no before at all is not a pass", {"EVENT": "push"}),
    ):
        r = run(env)
        expect(label,
               r.returncode == 1 and r.stdout == ""
               and r.stderr.startswith("SKIPPED (not a pass): push without a previous revision"),
               f"rc={r.returncode} out={r.stdout!r} err={r.stderr!r}")

    # --- an event nobody defined a range for is not a pass either ------------
    for label, env in (
        ("an unknown event is not a pass", {"EVENT": "schedule"}),
        ("no event at all is not a pass", {}),
    ):
        r = run(env)
        expect(label,
               r.returncode == 1 and r.stdout == ""
               and r.stderr.startswith("SKIPPED (not a pass): no range is defined for event"),
               f"rc={r.returncode} out={r.stdout!r} err={r.stderr!r}")

    if ran < MIN_CASES:
        fouten.append(f"only {ran} case(s) ran; the floor is {MIN_CASES}")

    if fouten:
        print(f"[test-internal-matter-range] FAIL: {len(fouten)} case(s):", file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"internal_matter_range: {ran} cases, all as intended")
    return 0


if __name__ == "__main__":
    sys.exit(main())
