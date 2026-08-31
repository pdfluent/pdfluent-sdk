#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
"""Every commit carries a DCO sign-off matching its author (#223).

WHY A SIGN-OFF AND NOT A CLA

A DCO certifies that the contributor had the right to submit the code. A CLA
additionally assigns us the right to relicense it commercially. Only the second
needs a lawyer, and there is no budget for one -- see
docs/contribution/why-a-dco-and-not-a-cla.md for the decision and the condition
that ends it.

WHY THE NAME HAS TO MATCH THE AUTHOR

A sign-off that names somebody else certifies nothing. `git commit -s` writes
the committer's own identity, so a mismatch is either a rebase that rewrote
authorship or a trailer pasted from another commit -- and both are exactly the
case where "who had the right to submit this" stops being answered.

The address is compared case-insensitively and nothing else about it is checked.
This gate is about provenance, not identity verification, and pretending
otherwise would be theatre.

FLOOR

An empty range passes every check without reading anything, which is
indistinguishable from a clean one. Below MIN_COMMITS the range itself is
treated as wrong.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys

MIN_COMMITS = 1
SIGNOFF = re.compile(r"^Signed-off-by:\s*(.+?)\s*<([^>]+)>\s*$", re.M | re.I)

# Commits that predate the decision on 31-08-2026 are not rewritten for it; see
# #261 and #230 on why this history is not being rewritten casually. The gate
# applies to what arrives from here on.
STANDAARD_BEREIK = "origin/master..HEAD"


def _omgeving() -> dict[str, str]:
    """git's own GIT_* variables leak into a hook's subprocesses; drop them."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def commits(bereik: str) -> list[tuple[str, str, str, str]]:
    uit = subprocess.run(
        ["git", "log", "--format=%H%x00%an%x00%ae%x00%B%x1e", bereik],
        capture_output=True, text=True, check=True, env=_omgeving()).stdout
    rijen = []
    for blok in uit.split("\x1e"):
        if not blok.strip():
            continue
        sha, naam, adres, boodschap = blok.strip("\n").split("\x00", 3)
        rijen.append((sha, naam, adres, boodschap))
    return rijen


def main(argv: list[str]) -> int:
    bereik = argv[1] if len(argv) > 1 else STANDAARD_BEREIK
    rijen = commits(bereik)
    if len(rijen) < MIN_COMMITS:
        print(f"[signoff] FATAL: {bereik} holds {len(rijen)} commit(s). An empty "
              "range passes without reading anything, which is not a clean range.",
              file=sys.stderr)
        return 1

    fouten: list[str] = []
    for sha, naam, adres, boodschap in rijen:
        gevonden = SIGNOFF.findall(boodschap)
        if not gevonden:
            fouten.append(f"{sha[:9]} has no Signed-off-by. `git commit -s --amend` "
                          "adds one; by adding it you certify docs/contribution/DCO.txt")
            continue
        if not any(a.strip().lower() == adres.strip().lower() for _, a in gevonden):
            ondertekenaars = ", ".join(a for _, a in gevonden)
            fouten.append(
                f"{sha[:9]} is authored by {adres} and signed off by "
                f"{ondertekenaars}. A sign-off naming somebody else certifies "
                "nothing about who had the right to submit this")

    if not fouten:
        print(f"[signoff] {len(rijen)} commit(s) in {bereik}; every one is signed off "
              "by its own author")
        return 0
    print(f"[signoff] {len(fouten)} commit(s) in {bereik} are not certified:",
          file=sys.stderr)
    for f in fouten:
        print(f"  - {f}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
