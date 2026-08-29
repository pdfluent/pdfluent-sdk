#!/usr/bin/env python3
"""Open pull requests are questions waiting for an answer, not an archive.

The same rule mr_staleness.py applies to GitLab, applied to GitHub -- because
GitHub is where the work moved, and the stack grew there unwatched. On
29-08-2026 there were fourteen open, six of them last touched in April: more
than four months and thousands of commits behind. Rebasing one of those is no
longer an afternoon, it is doing the work again.

The point is not tidiness. A stack of open pull requests looks like a queue and
is really an archive, and the difference is invisible until somebody tries to
merge one. One of the GitLab ones held a correct diagnosis of a CI clash that
went on costing pipeline runs for two and a half months while the fix sat in
it, unmerged.

Merge or close. Leaving open is not a third option: closing keeps the branch,
so it costs nothing except the appearance of something being in the queue.
"""
from __future__ import annotations

import datetime
import json
import os
import subprocess
import sys

REPO = os.environ.get("PDFLUENT_REPO", "jasperdew/xfa-native-rust")
# The thresholds already written down for GitLab. Same numbers on purpose:
# two rules with different limits is how one of them gets ignored.
WAARSCHUW_DAGEN, FAAL_DAGEN = 14, 30
WAARSCHUW_ACHTER, FAAL_ACHTER = 50, 200


# The stack as it stood on 29-08-2026, listed by number so it cannot grow
# quietly. These are being triaged under #282; a thirteenth failing here is a
# new one, and that is the whole point. Remove numbers as they merge or close.
BEKEND_OUD = {1034, 1271, 1272, 1278, 1322, 1349, 1350, 1397, 1398, 1465, 1469, 1471}


def gh(pad: str):
    try:
        r = subprocess.run(["gh", "api", pad], capture_output=True, text=True, check=False)
    except (FileNotFoundError, OSError):
        # gh missing is the ordinary case on a fresh runner, and a traceback
        # buries the one line that says what to install.
        return None
    if r.returncode != 0:
        return None
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return None


def main() -> int:
    prs = gh(f"repos/{REPO}/pulls?state=open&per_page=100")
    if prs is None:
        print("FAIL: could not read the pull request list. A stack nobody can see is "
              "exactly the state this check exists to prevent, so not being able to "
              "look is a failure, not a pass. Check that `gh` is installed and "
              "authenticated where this runs.", file=sys.stderr)
        return 1

    nu = datetime.datetime.now(datetime.timezone.utc)
    waarschuw, faal = [], []
    for pr in prs:
        bij = datetime.datetime.fromisoformat(pr["updated_at"].replace("Z", "+00:00"))
        dagen = (nu - bij).days
        vergelijk = gh(f"repos/{REPO}/compare/{pr['base']['ref']}...{pr['head']['sha']}")
        achter = (vergelijk or {}).get("behind_by")
        if achter is not None and achter >= FAAL_ACHTER and pr["number"] not in BEKEND_OUD:
            faal.append((pr["number"], dagen, f"{achter} commits behind — {pr['title'][:34]}"))
            continue
        if achter is not None and achter >= WAARSCHUW_ACHTER and dagen < WAARSCHUW_DAGEN:
            waarschuw.append((pr["number"], dagen,
                              f"{achter} commits behind — {pr['title'][:34]}"))
            continue
        if dagen >= FAAL_DAGEN and pr["number"] not in BEKEND_OUD:
            faal.append((pr["number"], dagen, pr["title"][:48]))
        elif dagen >= WAARSCHUW_DAGEN:
            waarschuw.append((pr["number"], dagen, pr["title"][:48]))

    resterend = sorted(BEKEND_OUD & {p["number"] for p in prs})
    print(f"[pr-staleness] {len(prs)} open pull request(s); "
          f"{len(resterend)} of the 29-08 backlog still open (#282)")
    for nr, dagen, titel in sorted(waarschuw, key=lambda r: -r[1]):
        print(f"  WARN  #{nr} untouched for {dagen} days — {titel}")
    for nr, dagen, titel in sorted(faal, key=lambda r: -r[1]):
        print(f"  STALE #{nr} untouched for {dagen} days — {titel}")

    if faal:
        print(file=sys.stderr)
        print(f"FAIL: {len(faal)} pull request(s) untouched for {FAAL_DAGEN} days or more. "
              "Rebase while that is still an afternoon, or close it -- closing keeps the "
              "branch, so it costs nothing but the illusion of a queue.", file=sys.stderr)
        return 1

    if not waarschuw:
        print("[pr-staleness] OK: nothing has been sitting long enough to rot.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
