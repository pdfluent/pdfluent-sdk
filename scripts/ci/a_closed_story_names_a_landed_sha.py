#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A story is closed with a commit master actually has (#348).

WHAT HAPPENED

Seven stories -- #161, #236, #237, #239, #246, #217, #219 -- were closed with
work that existed only on `chore/test-reachability-gate`, a branch that was
later split up and written off. Every one of them was closed with a commit sha
in the comment, and every one of those shas was real: they just were not on
master and never became so. The tracker said seven things were done and the
product had none of them, for two weeks, and it took a by-hand audit of a
different issue to notice.

A sha in a comment reads as proof. It is only proof of a commit existing
somewhere, which is a much weaker claim than the one it is taken for -- and the
gap between the two is invisible in exactly the way a green tick over an
untested path is invisible.

WHAT THIS CHECKS

For every issue closed since the cutoff: at least one sha named in the closing
comments is an ancestor of `master` in this repository.

A story can legitimately close without code in THIS repository -- superseded,
measured away, refuted, or landed in the website or the editor, which are
separate repositories this checkout cannot see. Both are allowed and both have
to be said rather than assumed:

    No code: superseded by #340, measured in the comment above
    Landed in pdfluent-website: 1823b4ae

The difference between "no code was needed here" and "the code never landed
anywhere" is the whole subject, so it is written down rather than inferred from
the absence of a sha.

IT REPORTS, IT DOES NOT REFUSE

Advisory in scripts/ci/local_ci_gate.sh, by owner decision on the same grounds
as the never-green guard (#331). What it watches is the TRACKER, and the tracker
is not in anybody's push: on its first run it fired over an issue another
terminal had closed twenty minutes earlier and refused a landing that had
nothing to do with it. A queue stopped by somebody else's state is the shape
that closed master four times in one day.

What that gives up is real and is said rather than implied: a closure without a
sha now goes unnoticed until somebody reads this output. What it does not give
up is the measurement, which is the half that was missing entirely -- the seven
closures above stood for two weeks with nothing looking at them at all.

Its TEST is a hard gate. That watches this repository's own code, which is in
the push, and it is the half that can be broken here.

WITHOUT ACCESS IT SAYS SO

Reading the tracker needs `gh` and a token, and comparing against master needs
this repository's history. A hosted runner has neither. No access is not "every
closure is sound"; it is no measurement, and it says SKIPPED (not a pass).

    python3 scripts/ci/a_closed_story_names_a_landed_sha.py
    ... --since 2026-09-01   # judge closures from another date

Exit codes:
    0  every closure since the cutoff names a landed sha, or a written reason
    1  one does not
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

REPO = os.environ.get("PDFLUENT_REPO", "pdfluent/engine")
TRACKER = os.environ.get("PDFLUENT_TRACKER", "pdfluent/pdfluent-internal")

# THE CUTOFF, and why there is one.
#
# The rule starts where it was written. Judging every closure ever made would
# report hundreds of issues closed before anyone had agreed to this, and a guard
# whose first run reports three hundred findings is a guard that gets switched
# off in the same afternoon. The seven that prompted it are reopened and land
# under the rule like anything else.
# It is the moment the rule landed rather than the start of that day: three
# stories closed earlier on 06-09-2026 name a sha this repository does not have,
# and all three are sound -- two landed in the website and the editor, which are
# separate repositories, and one is an epic closed on its children. They predate
# the two declarations below, and retro-judging them would refuse every push
# over comments written before the rule existed. Their state is recorded on
# #348 rather than hidden.
CUTOFF = "2026-09-06T10:00:00Z"

# A short sha is seven characters; a full one is forty. Below seven, ordinary
# words in a comment start matching -- `deface`, `added`, `feed`.
SHA = re.compile(r"\b[0-9a-f]{7,40}\b")

# The two written declarations. Both demand something after the colon: a bare
# `No code:` is the same silence with a label on it.
DECLARED = re.compile(r"^\s*(?:no code|landed in [^:\n]+):\s*\S", re.I | re.M)


def gh(path: str) -> tuple[object | None, str | None]:
    """(data, reason). Exactly one of the two is None."""
    try:
        done = subprocess.run(["gh", "api", path], capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return None, "`gh` is not installed on this machine"
    except OSError as exc:
        return None, f"`gh` could not be started: {exc}"
    if done.returncode != 0:
        first = ((done.stderr or done.stdout).strip().splitlines() or ["no output"])[0]
        return None, f"`gh api {path}` failed: {first}"
    try:
        return json.loads(done.stdout), None
    except json.JSONDecodeError as exc:
        return None, f"`gh api {path}` returned something that is not JSON: {exc}"


def _sealed() -> dict[str, str]:
    """The caller's environment without GIT_*.

    A hook exports GIT_DIR and GIT_WORK_TREE pointing at the real repository,
    and a git command that inherits them answers about that repository instead
    of the one it was pointed at -- so the same check passes by hand and fails
    inside the gate, on a different tree than the one it named.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def master() -> tuple[str | None, str | None]:
    """The ref this repository's master actually is, named rather than assumed.

    `github/master` first and `origin/master` second, the order every other
    guard here uses: origin pointed at the GitLab mirror on this machine while
    GitHub had been the source for a week, and a guard that quietly took the
    mirror compared against a repository 348 commits behind (#291).
    """
    for ref in ("github/master", "origin/master", "master"):
        try:
            done = subprocess.run(["git", "rev-parse", "--verify", "--quiet", ref],
                                  capture_output=True, text=True, check=False,
                                  env=_sealed())
        except OSError as exc:
            # Reported, not raised. A guard that dies on a missing binary refuses
            # the caller with a traceback instead of a verdict, which is how the
            # internal-terms guard stopped every commit on this machine on the
            # same day this was written.
            return None, f"`git` could not be started: {exc}"
        if done.returncode == 0:
            return ref, None
    return None, "this checkout has no master to compare against"


def is_ancestor(sha: str, ref: str) -> bool:
    done = subprocess.run(["git", "merge-base", "--is-ancestor", sha, ref],
                          capture_output=True, text=True, check=False,
                          env=_sealed())
    return done.returncode == 0


def closing_text(number: int) -> tuple[str, str | None]:
    """The issue body plus its comments.

    The sha is as often in the body -- edited on closing -- as in a comment, and
    a check that reads only one of the two turns a sound closure into a finding.
    """
    issue, reason = gh(f"repos/{TRACKER}/issues/{number}")
    if issue is None:
        return "", reason
    comments, reason = gh(f"repos/{TRACKER}/issues/{number}/comments?per_page=100")
    if comments is None:
        return "", reason
    parts = [issue.get("body") or ""]
    parts += [(c.get("body") or "") for c in comments]
    return "\n".join(parts), None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--since", default=CUTOFF, help=f"default {CUTOFF}")
    args = parser.parse_args()

    ref, reason = master()
    if ref is None:
        print(f"SKIPPED (not a pass): {reason}", file=sys.stderr)
        return 0

    issues, reason = gh(
        f"repos/{TRACKER}/issues?state=closed&since={args.since}&per_page=100")
    if issues is None:
        print(f"SKIPPED (not a pass): {reason}", file=sys.stderr)
        return 0
    if not isinstance(issues, list):
        print(f"unexpected answer from GitHub: {issues!r}", file=sys.stderr)
        return 1

    # `since` filters on UPDATED, not on closed, so a reopened or merely
    # commented-on issue arrives here too. Both are filtered out: a pull request
    # is not a story, and an issue closed before the cutoff is not judged.
    stories = [i for i in issues
               if "pull_request" not in i
               and (i.get("closed_at") or "") >= args.since]

    problems: list[str] = []
    unreadable = 0
    for issue in stories:
        text, reason = closing_text(issue["number"])
        if reason:
            unreadable += 1
            continue
        if DECLARED.search(text):
            continue
        landed = [s for s in set(SHA.findall(text)) if is_ancestor(s, ref)]
        if landed:
            continue
        named = sorted(set(SHA.findall(text)))
        problems.append(
            f"#{issue['number']} {issue['title'][:60]!r} — "
            + (f"names {len(named)} sha-like token(s), none of them an ancestor "
               f"of {ref}" if named else "names no sha and no `No code:` or "
               "`Landed in <repository>:` line")
        )

    if unreadable:
        print(f"NOTE: {unreadable} closed issue(s) could not be read, so they were "
              "not judged.", file=sys.stderr)

    if problems:
        print(f"[closed-story] {len(problems)} story/stories closed since "
              f"{args.since} without a commit {ref} has:", file=sys.stderr)
        for line in problems:
            print(f"  {line}", file=sys.stderr)
        print(
            "\nA sha in a comment proves a commit exists somewhere, which is a much\n"
            "weaker claim than the one it is read as. Seven stories were closed that\n"
            "way on a branch that was later written off (#348).\n"
            "\nFix: comment the landing sha once it is on master; or, if the work\n"
            "landed elsewhere or needed no code, say so with a line beginning\n"
            "`Landed in <repository>:` or `No code:`.",
            file=sys.stderr,
        )
        return 1

    print(f"[closed-story] OK: {len(stories)} story/stories closed since "
          f"{args.since}; each names a commit {ref} has, or a written reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
