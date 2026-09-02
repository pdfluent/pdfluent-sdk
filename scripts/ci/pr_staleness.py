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
    """(data, reason). Exactly one of the two is None.

    Four different failures used to arrive here as the same `None`: the binary
    missing, a non-zero exit, a network error and unparseable output. The
    caller then printed one guess for all four -- "check that `gh` is installed
    and authenticated" -- and on 02-09-2026 that guess was wrong: seven agents
    had exhausted the GitHub API rate limit, and the gate said the operator was
    not logged in. A guard that reports a cause it did not measure sends people
    to fix the wrong thing, which costs more than saying "I could not tell".
    """
    try:
        r = subprocess.run(["gh", "api", pad], capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return None, "`gh` is not installed on this machine"
    except OSError as e:
        return None, f"`gh` could not be started: {e}"
    if r.returncode != 0:
        # gh's own words. It distinguishes a rate limit from a login problem
        # from a 404, and this guard does not have to guess between them.
        melding = (r.stderr or r.stdout).strip().splitlines()
        eerste = melding[0] if melding else f"exit {r.returncode} with no output"
        return None, f"`gh api {pad}` failed: {eerste}"
    try:
        return json.loads(r.stdout), None
    except json.JSONDecodeError as e:
        return None, f"`gh api {pad}` returned something that is not JSON: {e}"


def main() -> int:
    prs, reden = gh(f"repos/{REPO}/pulls?state=open&per_page=100")
    if prs is None:
        print(f"FAIL: could not read the pull request list -- {reden}\n"
              "A stack nobody can see is exactly the state this check exists to "
              "prevent, so not being able to look is a failure and not a pass. "
              "The line above is what the tool said, not a guess: a rate limit, "
              "a missing login and a missing binary need three different fixes.",
              file=sys.stderr)
        return 1

    nu = datetime.datetime.now(datetime.timezone.utc)
    waarschuw, faal = [], []
    for pr in prs:
        bij = datetime.datetime.fromisoformat(pr["updated_at"].replace("Z", "+00:00"))
        dagen = (nu - bij).days
        vergelijk, _ = gh(f"repos/{REPO}/compare/{pr['base']['ref']}...{pr['head']['sha']}")
        achter = (vergelijk or {}).get("behind_by")
        if achter is None:
            # The compare call failed. Falling back to the day count alone lets
            # a pull request that was commented on yesterday and is two thousand
            # commits behind read as healthy -- which is exactly the case the
            # commit thresholds exist for.
            print(f"  ?     #{pr['number']} could not be compared against "
                  f"{pr['base']['ref']}; commit distance unknown")
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

    # Blocking only where blocking helps. On a pull request this runs beside the
    # other gates, and one pull request crossing thirty days would stop every
    # merge in the repository -- including the merges that would clear the
    # backlog. A guard against neglect that freezes the work is worse than the
    # neglect. Set PR_STALENESS_BLOCKING=1 in the scheduled run, where failing
    # costs nobody their afternoon.
    blokkerend = os.environ.get("PR_STALENESS_BLOCKING") == "1"
    if faal and not blokkerend:
        print(f"[pr-staleness] {len(faal)} stale pull request(s); reported, not "
              "blocking here. The scheduled run fails on these.")
        return 0
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
