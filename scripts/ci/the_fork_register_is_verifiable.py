#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""`gelijk_met` must be checkable against a commit, not asserted by hand.

`upstream_has_not_moved_on.py` decides whether a fork is behind by comparing
`gelijk_met` in docs/UPSTREAM_FORKS.toml against crates.io. That makes the
register the only input, and on 31-08-2026 the register was wrong: `hayro-ccitt`
claimed to be level with upstream 0.3.0 while the code sat on the 0.2.0 fork
point, seven commits back. The guard reported it as current, because a guard is
exactly as honest as its baseline and this baseline was typed in by a person.

So each entry now records the upstream COMMIT it corresponds to, and this check
verifies that the version declared in that commit's Cargo.toml is the version
claimed. A typo, a stale entry or an optimistic update fails here instead of
becoming a fact the other guard repeats.

It needs a clone of upstream, which CI may not have. That case announces itself
rather than passing: a check that cannot reach its evidence has not checked.

Exit codes:
  0  every entry with a fork point matches that commit
  1  an entry claims a version its fork point does not carry
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTER = ROOT / "docs/UPSTREAM_FORKS.toml"

# Where a clone of LaurenzV/hayro can be found. CI may set this; a developer
# usually has one lying around from the last upgrade.
CLONE = Path(os.environ.get("HAYRO_CLONE", "/tmp/hayro-up"))


def version_at(commit: str, crate_dir: str) -> str | None:
    out = subprocess.run(
        ["/usr/bin/git", "show", f"{commit}:{crate_dir}/Cargo.toml"],
        cwd=CLONE,
        capture_output=True,
        text=True,
        check=False,
    )
    if out.returncode != 0:
        return None
    for line in out.stdout.splitlines():
        if line.startswith("version"):
            return line.split('"')[1]
    return None


def main() -> int:
    if not REGISTER.exists():
        print(f"SKIPPED (not a pass): {REGISTER} is missing", file=sys.stderr)
        return 3

    forks = tomllib.loads(REGISTER.read_text()).get("fork", [])
    with_point = [f for f in forks if f.get("forkpunt")]

    if not with_point:
        print(
            "SKIPPED (not a pass): no entry in UPSTREAM_FORKS.toml records a fork point,\n"
            "  so there is nothing to verify `gelijk_met` against.",
            file=sys.stderr,
        )
        return 3

    if not (CLONE / ".git").is_dir():
        print(
            f"SKIPPED (not a pass): no clone of LaurenzV/hayro at {CLONE}.\n"
            "  Set HAYRO_CLONE, or clone it there. Without upstream's history the\n"
            "  register's claims cannot be checked, only repeated.",
            file=sys.stderr,
        )
        return 3

    problems: list[str] = []
    checked = 0
    for f in with_point:
        crate, point, claimed = f["onze_crate"], f["forkpunt"], f["gelijk_met"]
        actual = version_at(point, f["upstream"])
        if actual is None:
            problems.append(
                f"{crate}: fork point {point} does not resolve, or has no {f['upstream']}/Cargo.toml. "
                "The register points at a commit that is not there."
            )
            continue
        checked += 1
        if actual != claimed:
            problems.append(
                f"{crate}: register says gelijk_met = {claimed}, but {point} carries {actual}. "
                "One of the two is wrong, and the other guard trusts this one."
            )

    if problems:
        print(f"Fork register: {len(problems)} entry/entries do not match their fork point\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1

    print(f"✓ {checked} fork entry/entries match the version at their recorded fork point")
    return 0


if __name__ == "__main__":
    sys.exit(main())
