#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""A branch far ahead of master with no merge request is invisible.

`mr_staleness.py` warns from 14 days or 50 commits behind and fails from 30 days
or 200. It reads merge requests, so a branch that never got one is not late in
its eyes -- it does not exist at all.

On 28-08-2026 that turned out to matter: `chore/test-reachability-gate` stood
295 commits ahead of master, 993 files and 66k lines, with no merge request
anywhere. Fifteen finished pieces of work, none of them on master, and nothing
counting them (#272).

The rule in CLAUDE.md is that a merge request is a question waiting for an
answer. A branch without one is not even a question.

# NO-FLOOR: this guard discovers nothing it could stop finding. It asks one
# question about one branch -- the checked-out one -- and the ahead-count comes
# from git, which fails loudly rather than returning zero.
"""

from __future__ import annotations

import json
import subprocess
import sys

# A branch this far ahead is a body of work, not a fix in progress.
WAARSCHUW_VANAF = 50
FAAL_VANAF = 150


def draai(*args: str) -> str | None:
    try:
        r = subprocess.run(args, capture_output=True, text=True, check=False, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def main() -> int:
    tak = draai("git", "rev-parse", "--abbrev-ref", "HEAD")
    if not tak or tak == "HEAD":
        print("SKIPPED (not a pass): detached HEAD, there is no branch to ask about.",
              file=sys.stderr)
        return 0

    standaard = None
    for kandidaat in ("github/master", "origin/master", "github/main", "origin/main"):
        if draai("git", "rev-parse", "--verify", "--quiet", kandidaat):
            standaard = kandidaat
            break
    if standaard is None:
        print("SKIPPED (not a pass): no master/main remote ref to compare against.",
              file=sys.stderr)
        return 0

    # Not `tak.split("/")[-1]`: that reads `release/main` and `feature/master`
    # as the default branch and skips a topic branch entirely. Codex, #1542.
    if tak in ("master", "main"):
        print(f"[branch-mr] on {tak}; nothing to ask.")
        return 0

    vooruit = draai("git", "rev-list", "--count", f"{standaard}..HEAD")
    if vooruit is None:
        print("SKIPPED (not a pass): git rev-list failed; cannot count the branch.",
              file=sys.stderr)
        return 0
    vooruit = int(vooruit)

    if vooruit < WAARSCHUW_VANAF:
        print(f"[branch-mr] {tak} is {vooruit} commit(s) ahead of {standaard}.")
        return 0

    uit = draai("gh", "pr", "list", "--head", tak, "--state", "all",
                "--json", "number,state", "--limit", "5")
    if uit is None:
        print(
            f"SKIPPED (not a pass): {tak} is {vooruit} commits ahead and `gh` is not "
            "available here, so whether it has a merge request could not be checked.",
            file=sys.stderr,
        )
        return 0
    try:
        prs = json.loads(uit or "[]")
    except json.JSONDecodeError:
        print("SKIPPED (not a pass): `gh pr list` returned something unreadable.",
              file=sys.stderr)
        return 0

    if prs:
        staat = ", ".join(f"#{p['number']} {p['state']}" for p in prs)
        print(f"[branch-mr] {tak} is {vooruit} ahead and has {staat}.")
        return 0

    # A branch that is not on the remote yet cannot have a merge request, and
    # telling it to open one is advice it cannot take: `gh pr create` needs a
    # remote branch, and the only way to get one is the push this refuses.
    #
    # T3 hit this resolving #1543. Their branch was 323 commits ahead -- inherited
    # from the branch they were resolving, not work they had piled up -- and was
    # being pushed AT `chore/test-reachability-gate`, which has had PR #1543 open
    # for days. The work was never going to be invisible; the guard simply judged
    # the wrong branch, and then demanded the impossible.
    #
    # A guard that cannot be satisfied is not a standard, it is an obstacle, and
    # the only way past it is `PRE_PUSH_SKIP=1` -- which skips the other 32 gates
    # too. Refusing to be satisfiable is how a gate teaches people to bypass it.
    #
    # So: still fatal for a branch that IS on the remote and far ahead with no
    # merge request, which is the case #272 was about. Not fatal for one that has
    # never been pushed, where the demand is unmeetable by construction.
    op_de_remote = draai("git", "rev-parse", "--verify", "--quiet",
                         f"github/{tak}") or draai(
                         "git", "rev-parse", "--verify", "--quiet", f"origin/{tak}")
    if not op_de_remote:
        print(f"[branch-mr] {tak} is {vooruit} ahead of {standaard} and is not on the "
              "remote yet, so it cannot have a merge request. Push it, then open one "
              "-- and if it is still without one on the next push, this fails.")
        return 0

    ernst = "FATAL" if vooruit >= FAAL_VANAF else "WARNING"
    print(f"[branch-mr] {ernst}: {tak} is {vooruit} commits ahead of {standaard} "
          "and has no merge request.", file=sys.stderr)
    print(
        "\nmr_staleness.py cannot see this: it reads merge requests, so a branch "
        "that never got one is not late in its eyes, it is absent. Open one now, "
        "while the rebase is still an afternoon -- or say on the branch's issue why "
        "it is deliberately parked. (#272)",
        file=sys.stderr,
    )
    return 1 if vooruit >= FAAL_VANAF else 0


if __name__ == "__main__":
    raise SystemExit(main())
