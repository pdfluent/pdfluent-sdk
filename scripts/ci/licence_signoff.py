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
import pathlib
import re
import subprocess
import sys

MIN_COMMITS = 1

# The cutoff is IMPORTED, not repeated. This file's comment promised that "the
# gate applies to what arrives from here on" and then walked the whole range, so
# on a long-lived branch it demanded a sign-off from every commit that branch
# ever carried -- 340 on #1543, none written after the decision and none of them
# this push's to certify. A promise a comment makes and the code does not keep is
# worse than no promise: it reads as a bound.
#
# Copying the number would leave two answers to one question, which is the defect
# the register guards exist to catch. every_commit_since_the_cutoff_is_signed.py
# defines it; this reads it.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from every_commit_since_the_cutoff_is_signed import CUT_AT  # noqa: E402
SIGNOFF = re.compile(r"^Signed-off-by:\s*(.+?)\s*<([^>]+)>\s*$", re.M | re.I)

# Commits that predate the decision on 31-08-2026 are not rewritten for it; see
# #261 and #230 on why this history is not being rewritten casually. The gate
# applies to what arrives from here on.
#
# `github`, not `origin`. `origin` in this checkout is the GitLab mirror, which
# was 166 commits behind GitHub when this line was corrected -- so the default
# range would have covered every commit master gained since the mirror last ran
# and reported all of them unsigned. #291 records the same defect in
# territories_do_not_overlap.py, which called 62 files a branch's work when none
# were, and in mr_staleness.py, which still has it.
#
# A range that names the wrong remote does not fail; it answers a different
# question confidently.
def _standaard_bereik() -> str:
    for remote in ("github/master", "origin/master"):
        r = subprocess.run(["git", "rev-parse", "--verify", "--quiet", remote],
                           capture_output=True, text=True, env=_omgeving())
        if r.returncode == 0:
            return f"{remote}..HEAD"
    return "HEAD~1..HEAD"


def _omgeving() -> dict[str, str]:
    """git's own GIT_* variables leak into a hook's subprocesses; drop them."""
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def commits(bereik: str) -> list[tuple[str, str, str, str]]:
    uit = subprocess.run(
        # `--no-merges`: a merge carries a generated message and can never hold a
        # sign-off, and on a pull_request event the checkout resolves HEAD to the
        # synthetic merge GitHub builds -- demanding one of it makes the gate
        # unsatisfiable rather than strict. Same shape #1635 repaired in the
        # deletion guard and #1683 in the sign-off job.
        #
        # `%at` is the AUTHOR date. The committer date moves on every rebase, so
        # using it would drag old commits across the cutoff and demand certification
        # for work that predates the decision.
        ["git", "log", "--no-merges",
         "--format=%H%x00%at%x00%an%x00%ae%x00%B%x1e", bereik],
        capture_output=True, text=True, check=True, env=_omgeving()).stdout
    rijen = []
    for blok in uit.split("\x1e"):
        if not blok.strip():
            continue
        sha, at, naam, adres, boodschap = blok.strip("\n").split("\x00", 4)
        rijen.append((sha, int(at), naam, adres, boodschap))
    return rijen


def main(argv: list[str]) -> int:
    bereik = argv[1] if len(argv) > 1 else _standaard_bereik()
    alle = commits(bereik)
    if len(alle) < MIN_COMMITS:
        print(f"[signoff] FATAL: {bereik} holds {len(alle)} commit(s). An empty "
              "range passes without reading anything, which is not a clean range.",
              file=sys.stderr)
        return 1
    
    # The floor above is about the RANGE; this is about what the rule covers.
    # Empty here is a real answer -- "this push adds nothing written after the
    # decision" -- and must not be confused with a range that read nothing.
    rijen = [(s, n, a, b) for s, at, n, a, b in alle if at > CUT_AT]
    if not rijen:
        print(f"[signoff] OK: {bereik} adds no commit written after the cutoff; "
              "sign-off not required.")
        return 0
    
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
