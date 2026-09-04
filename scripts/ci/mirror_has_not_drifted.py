#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A mirror that quietly stopped looks exactly like one with nothing to do.

GitHub is the source; GitLab is a backup of it. That only means anything while
the two are the same. On 27-08-2026 they had been apart since the 21st: GitHub
carried 18 commits of CI work, GitLab carried 101 commits of product work, and
neither was a superset of the other. Six days, and nothing said so.

Worse, it made the backup dangerous rather than merely stale: a `push --mirror`
in either direction would have destroyed the far side's work. The thing meant to
protect the repository could not be used on it (#265, #231).

Two thresholds, because drift arrives in two shapes. A backup that is a few
commits behind is a backup. One that is fifty behind, or two days old, is a
snapshot of something else.

The direction matters as much as the distance. GitLab being behind is the mirror
lagging. GitLab being *ahead* means someone is still working there, which is the
thing the switchover on 28-08 was meant to end -- so any amount of that fails.

# NO-FLOOR: it compares two refs. There is nothing here it could find less of;
# a ref it cannot read is announced, never counted as agreement.
"""

from __future__ import annotations

import datetime
import os
import subprocess
import sys

def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A pre-push hook exports GIT_DIR and GIT_INDEX_FILE, and a subprocess
    inherits them: a git command meant for a scratch directory then operates on
    the real repository. That put `core.bare = true` on this one and stopped
    thirty worktrees -- damage outside the script, not a wrong answer inside it.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


ACHTER_COMMITS = 50
ACHTER_DAGEN = 2

BRON = os.environ.get("MIRROR_SOURCE", "github/master")
SPIEGEL = os.environ.get("MIRROR_TARGET", "origin/master")


def git(*args: str) -> str | None:
    try:
        r = subprocess.run(["git", *args], capture_output=True, text=True,
                           check=False, timeout=120, env=schone_omgeving())
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def main() -> int:
    for ref in (BRON, SPIEGEL):
        if git("rev-parse", "--verify", "--quiet", ref) is None:
            print(f"SKIPPED (not a pass): {ref} cannot be read here, so nothing was "
                  "compared. Fetch both remotes first.", file=sys.stderr)
            return 0

    achter = git("rev-list", "--count", f"{SPIEGEL}..{BRON}")
    voor = git("rev-list", "--count", f"{BRON}..{SPIEGEL}")
    if achter is None or voor is None:
        print("SKIPPED (not a pass): git rev-list failed; no comparison was made.",
              file=sys.stderr)
        return 0
    achter, voor = int(achter), int(voor)

    klachten: list[str] = []

    if voor > 0:
        klachten.append(
            f"{SPIEGEL} is {voor} commit(s) AHEAD of {BRON}. A backup does not gain "
            "commits of its own -- someone is still working there, and mirroring in "
            "the agreed direction would destroy that work. This is how the two "
            "masters came apart for six days (#265)"
        )

    if achter > ACHTER_COMMITS:
        klachten.append(
            f"{SPIEGEL} is {achter} commit(s) behind {BRON}, over the {ACHTER_COMMITS} "
            "allowed. That is not a lagging backup, it is a snapshot of something else"
        )

    laatste = git("log", "-1", "--format=%cI", SPIEGEL)
    if laatste:
        toen = datetime.datetime.fromisoformat(laatste)
        dagen = (datetime.datetime.now(datetime.timezone.utc) - toen).days
        if achter > 0 and dagen > ACHTER_DAGEN:
            klachten.append(
                f"{SPIEGEL} last moved {dagen} day(s) ago and is {achter} behind. A "
                "mirror that quietly stopped looks exactly like one with nothing to do"
            )
        print(f"[mirror] {SPIEGEL}: {achter} behind, {voor} ahead, last moved "
              f"{dagen} day(s) ago")
    else:
        print(f"[mirror] {SPIEGEL}: {achter} behind, {voor} ahead")

    if not klachten:
        print("[mirror] OK: the backup is a backup.")
        return 0

    print(file=sys.stderr)
    print(f"[mirror] FATAL: {len(klachten)} problem(s) with the mirror:", file=sys.stderr)
    for k in klachten:
        print(f"  - {k}", file=sys.stderr)
    print(
        "\nGitHub is the source and GitLab is a copy of it, which only means something "
        "while the two agree. Push the source to the mirror, or say on #231 why the "
        "direction changed.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
