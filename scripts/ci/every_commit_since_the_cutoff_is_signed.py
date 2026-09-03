#!/usr/bin/env python3
"""Every commit this push adds since the #316 cutoff carries a sign-off.

This is the layer that decides. master only ever takes fast-forwards, so the
pre-push gate IS the merge point: what passes here is what lands. The CI job of
the same name runs on a pull_request event, where GitHub hands the checkout a
synthetic merge commit and the range is the pull request's own commits -- useful,
but not the thing that gates the merge.

WHY THE RANGE IS "ON NO REMOTE REF" AND NOT `@{u}..HEAD`

The rule covers commits written after the cutoff, and this push only introduces
the ones the remote does not already have. Asking about the rest would report
somebody else's unsigned commit as this push's problem, and the decision on #316
is explicit that history before the cutoff stays uncertified rather than being
signed retroactively.

`@{u}..HEAD` looked like that question and was not. After a rebase the upstream
ref still points at the PRE-rebase head, so the range stops meaning "what this
push adds" and starts meaning "everything the new base has that the old head did
not" -- which is master's own recent history. Measured on a branch that was
master+2: `@{u}..HEAD` held 10 commits and the gate refused 7, six of which were
already on master, verified one by one with `git merge-base --is-ancestor`. A
push cannot be answerable for commits that are already in the repository.

It does not clear on its own either: master carries commits written after the
cutoff that predate the gate, so every branch rebased onto master inherits them
into that range. On 03-09 that was 8 of the last 300 non-merge commits, which
made every rebased push unpushable at once.

Asking "which commits are on no remote ref" is the question that was meant. It
survives a rebase, and it does not depend on a branch NAME -- the other way this
repository has been misled today, where querying one name returned "absent" for
a branch that was present under a different one.

A stale remote-tracking ref makes this stricter, never laxer: an unfetched ref
means a commit looks new when it is not, which asks for a sign-off that is
already there. The failure direction of a missing fetch is a demand, not a pass.

WHY NO REMOTE REFS AT ALL IS NOT A PASS

"No refs to measure against" is indistinguishable from "nothing to check" --
which is how a gate reports success over an empty question. It falls back to the
whole branch, so a first push into an empty remote is covered rather than waved
through.
"""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

# The repository this runs IN, not the one this file lives in. A guard that
# derives its target from its own location always inspects its own checkout: it
# cannot be pointed at a fixture, so it cannot be tested, and when it is vendored
# somewhere it answers about the wrong tree. Pre-push runs it in the repository
# being pushed, which is exactly what `--show-toplevel` reports.
def _repo() -> Path:
    uit = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True, check=False,
                         env={k: v for k, v in os.environ.items()
                              if not k.startswith("GIT_")})
    if uit.returncode != 0 or not uit.stdout.strip():
        print("[signoff-push] SKIPPED (not a pass): not inside a git repository, "
              "so there is no range to read.", file=sys.stderr)
        raise SystemExit(1)
    return Path(uit.stdout.strip())

# The author date of the commit carrying out the owner's decision on #316. The
# same moment ci.yml uses; the two are meant to stay equal, and
# test_every_commit_since_the_cutoff_is_signed.py fails when they drift.
CUT_AT = 1788420644


REPO = None  # set in main(), see _repo()


def schone_omgeving() -> dict[str, str]:
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True,
                          text=True, env=schone_omgeving(), check=False)


def main() -> int:
    global REPO
    REPO = _repo()
    # Every remote-tracking ref, not one branch's upstream and not a name we
    # guessed. A commit reachable from any of them is published already, so this
    # push is not what introduces it.
    refs = git("for-each-ref", "--format=%(refname)", "refs/remotes/")
    if refs.returncode == 0 and refs.stdout.strip():
        argumenten = ["HEAD", "--not", "--remotes"]
        bereik = "commits on no remote ref"
    else:
        # Nothing published to compare against -- a fresh remote, or a fixture.
        # Everything the branch holds is what a push would introduce. The honest
        # range, rather than a refusal or a wave-through.
        argumenten = ["HEAD"]
        bereik = "the whole branch (no remote refs to measure against)"

    # `--no-merges` for the reason ci.yml carries at length: a merge commit has
    # a generated message and cannot carry a sign-off, so demanding one of it
    # makes the gate unsatisfiable rather than strict.
    uit = git("log", "--no-merges", "--format=%H %at", *argumenten)
    if uit.returncode != 0:
        print(f"[signoff-push] SKIPPED (not a pass): `git log {' '.join(argumenten)}` "
              f"failed:\n"
              f"  {uit.stderr.strip()[:200]}", file=sys.stderr)
        return 1

    na_cutoff = [r.split()[0] for r in uit.stdout.splitlines()
                 if r.strip() and int(r.split()[1]) > CUT_AT]
    if not na_cutoff:
        print(f"[signoff-push] OK: {bereik} -- none written after the cutoff; "
              "sign-off not required.")
        return 0

    ongetekend = []
    for sha in na_cutoff:
        body = git("log", "-1", "--format=%B", sha).stdout
        if "signed-off-by:" not in body.lower():
            onderwerp = git("log", "-1", "--format=%s", sha).stdout.strip()
            ongetekend.append((sha[:9], onderwerp))

    if not ongetekend:
        print(f"[signoff-push] OK: {len(na_cutoff)} commit(s) after the cutoff, "
              "each carrying a sign-off")
        return 0

    print(f"[signoff-push] {len(ongetekend)} of {len(na_cutoff)} commit(s) this "
          "push adds since the cutoff carry no sign-off:\n", file=sys.stderr)
    for sha, onderwerp in ongetekend:
        print(f"  - {sha}  {onderwerp[:70]}", file=sys.stderr)
    print("\n  Add it with `git commit -s --amend` on the last one, or for a\n"
          "  range: git rebase <base> --exec 'git commit --amend --no-edit -s'\n"
          "\n  By adding it you certify docs/contribution/DCO.txt. The decision on\n"
          "  #316 is that commits written by the owner's agents under his account\n"
          "  carry his sign-off, because the account, the review and the merge are\n"
          "  his -- it is not a formality this gate can add for you.",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
