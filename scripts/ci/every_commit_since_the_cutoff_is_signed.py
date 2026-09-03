#!/usr/bin/env python3
"""Every commit this push adds since the #316 cutoff carries a sign-off.

This is the layer that decides. master only ever takes fast-forwards, so the
pre-push gate IS the merge point: what passes here is what lands. The CI job of
the same name runs on a pull_request event, where GitHub hands the checkout a
synthetic merge commit and the range is the pull request's own commits -- useful,
but not the thing that gates the merge.

WHY THE RANGE IS `@{u}..HEAD` AND NOT THE CUTOFF TO HEAD

The rule covers commits written after the cutoff, and this push only introduces
the ones the upstream branch does not have. Asking about the rest would report
somebody else's unsigned commit as this push's problem, and the decision on #316
is explicit that history before the cutoff stays uncertified rather than being
signed retroactively.

WHY A MISSING UPSTREAM IS NOT A PASS

A branch with no upstream has no range, and "no range" is indistinguishable from
"nothing to check" -- which is how a gate reports success over an empty
question. Here it falls back to the cutoff, so a first push of a new branch is
covered rather than waved through.
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
    boven = git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
    if boven.returncode == 0 and boven.stdout.strip():
        bereik = f"{boven.stdout.strip()}..HEAD"
    else:
        # No upstream: this is a branch nobody has seen. Everything on it that
        # is not on master is what the push introduces.
        bereik = ""
        for ref in ("github/master", "origin/master"):
            basis = git("merge-base", "HEAD", ref)
            if basis.returncode == 0 and basis.stdout.strip():
                bereik = f"{basis.stdout.strip()}..HEAD"
                break
        if not bereik:
            # No upstream and no master to measure against: everything this
            # branch contains is what a push would introduce. That is the honest
            # range rather than a refusal -- and it is the case a brand-new
            # repository is in, which is also the case every fixture is in.
            bereik = "HEAD"

    # `--no-merges` for the reason ci.yml carries at length: a merge commit has
    # a generated message and cannot carry a sign-off, so demanding one of it
    # makes the gate unsatisfiable rather than strict.
    uit = git("log", "--no-merges", "--format=%H %at", bereik)
    if uit.returncode != 0:
        print(f"[signoff-push] SKIPPED (not a pass): `git log {bereik}` failed:\n"
              f"  {uit.stderr.strip()[:200]}", file=sys.stderr)
        return 1

    na_cutoff = [r.split()[0] for r in uit.stdout.splitlines()
                 if r.strip() and int(r.split()[1]) > CUT_AT]
    if not na_cutoff:
        print(f"[signoff-push] OK: {bereik} adds no commit written after the "
              "cutoff; sign-off not required.")
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
