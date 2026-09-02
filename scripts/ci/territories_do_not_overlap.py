#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Several terminals, one repository, no collisions -- enforced rather than agreed.

Allocating issues is not enough. Two terminals can hold different issues and
still edit the same file, which is how the collisions actually happen. So the
unit of ownership is a PATH, declared in .claude/territories.toml, and a branch
declares which territory it is working in through its name: `t2/ci-guards`
belongs to t2.

Two checks, and the second is the one that earns its place:

1. No two territories claim the same path. An overlap in the map is a collision
   waiting for the day both terminals are busy.
2. A branch has not changed files outside the territory it named. This is what a
   convention cannot do: it catches the edit that seemed harmless at the time.

Working outside your territory is allowed, but not quietly: change
.claude/territories.toml in a commit, so it comes past review.

Exit codes:
  0  the map is consistent and the branch stayed inside it
  1  two territories overlap, or the branch reached outside its own
  3  cannot check (announced, never silent)
"""

from __future__ import annotations

import fnmatch
import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAP = ROOT / ".claude/territories.toml"
GIT = "/usr/bin/git"


def load() -> list[dict]:
    """The map AS COMMITTED, falling back to the working tree only if there is
    no commit to read.

    Reading the file on disk made claiming free. A branch in t1 could touch a
    path t2 owns, add the claim to `.claude/territories.toml`, leave it
    uncommitted, and pass: the guard read the edit, agreed the path was claimed,
    and exited 0 -- while nothing about that claim ever reached a commit, a
    diff, or a reviewer. The docstring already promised the opposite ("taking on
    work in another territory is a commit, not a silent edit"), and a promise
    the code does not keep is the failure this repository keeps finding.

    The fallback is for the one case with no commit to read -- a fixture's
    freshly initialised repository before its first commit -- and it is narrow
    on purpose: `git show` failing for any other reason leaves the guard reading
    what it is supposed to be checking. (peer review, #1636)
    """
    uit = subprocess.run([GIT, "show", f"HEAD:{MAP.relative_to(ROOT)}"],
                         cwd=ROOT, capture_output=True, text=True, check=False)
    tekst = uit.stdout if uit.returncode == 0 else MAP.read_text()
    return tomllib.loads(tekst).get("territory", [])


def owns(territory: dict, path: str) -> bool:
    for pattern in territory.get("uitgezonderd", []):
        if fnmatch.fnmatch(path, pattern):
            return False
    return any(fnmatch.fnmatch(path, p) for p in territory.get("paden", []))


def changed_files() -> tuple[list[str], str] | None:
    """Diff against the PRIMARY remote's master, and say which one that was.

    GitHub has been primary since 25-08-2026 and GitLab is a nightly mirror that
    runs behind. Diffing against the mirror shows every commit the mirror has not
    received yet as if this branch had made it -- the first run of this check
    reported 62 files that were not mine. mr_staleness.py had the same bug.
    """
    for base in ("github/master", "origin/master"):
        exists = subprocess.run([GIT, "rev-parse", "--verify", "--quiet", base],
                                cwd=ROOT, capture_output=True, text=True, check=False)
        if exists.returncode != 0:
            continue
        out = subprocess.run([GIT, "diff", "--name-only", f"{base}...HEAD"],
                             cwd=ROOT, capture_output=True, text=True, check=False)
        if out.returncode == 0:
            return [line for line in out.stdout.splitlines() if line], base
    return None


class Ambigu(Exception):
    """Several territory refs point at this commit; no one of them is the answer."""

    def __init__(self, namen: list[str]) -> None:
        super().__init__(", ".join(namen))
        self.namen = namen


def current_branch() -> str | None:
    """The branch this work is on, or None if it genuinely cannot be known.

    `rev-parse --abbrev-ref HEAD` answers "HEAD" on a detached checkout, and
    "HEAD" contains no slash, so the branch half of this guard skipped itself and
    the run still printed the green overlap line. Anyone reading that line read
    "the gate is green" for a check that had run half of itself.

    That is not hypothetical and it is not rare: every worktree in the #1543
    relay on 01-09-2026 was detached, so every territory claim made that day was
    a half measurement -- including the ones I made about my own paths. Reported
    by codex as a P1 on #296, and independently rediscovered by t3 with a better
    measurement than the original find.

    So: ask git, then ask the CI environment, then ask which branches point here.
    If none of them answers, the caller must say so out loud rather than pass.
    """
    out = subprocess.run(
        [GIT, "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )
    name = out.stdout.strip() if out.returncode == 0 else ""
    if name and name != "HEAD":
        return name

    # CI checks out a detached commit and puts the name in the environment.
    for var in ("GITHUB_HEAD_REF", "GITHUB_REF_NAME", "CI_COMMIT_REF_NAME",
                "TERRITORY_BRANCH"):
        value = os.environ.get(var, "").strip()
        if value and value != "HEAD":
            return value

    # A worktree detached at a commit a branch still points to -- the shape every
    # relay worktree had. Prefer a name that looks like a territory claim; a
    # commit can carry several refs and only one of them answers this question.
    # Local branches AND remote-tracking refs. A CI checkout is detached at a
    # commit whose only ref is `refs/remotes/github/<branch>`, so asking `git
    # branch` alone asked the one question the CI shape cannot answer -- which
    # is the shape this whole function exists for. Tags are left out: a tag is
    # not a claim of ownership and names no territory.
    pointing = subprocess.run(
        [GIT, "for-each-ref", "--points-at", "HEAD", "--format=%(refname:short)",
         "refs/heads", "refs/remotes"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    )
    # `--points-at` also prints git's pseudo-entry "(HEAD detached at <sha>)".
    # Taking that as a branch name produced exit 3 with a message about a branch
    # nobody named, which is the right verdict reached by the wrong route -- and a
    # verdict reached by the wrong route stops being right the moment the route
    # changes.
    names = [n.strip() for n in pointing.stdout.splitlines()
             if n.strip() and not n.startswith("(")]

    # Prefer a name whose prefix is an actual territory. A commit can carry
    # several branches -- `t2/ci-fix` and `feature/alias` both pointing here --
    # and taking whichever git listed first would judge the work against a
    # territory nobody named, or against none. The gate would then reject a
    # correct change for belonging to the wrong owner, which is worse than not
    # checking: it teaches people the guard is unreliable. (codex, #1636)
    known = {t["id"] for t in load()}
    # `github/t2/ci-fix` is the same claim as `t2/ci-fix`: the remote name is
    # not a territory, so it is dropped before the prefix is read.
    def zonder_remote(n: str) -> str:
        deel = n.split("/")
        if len(deel) > 2 and deel[0] not in known and deel[1] in known:
            return "/".join(deel[1:])
        return n

    names = [zonder_remote(n) for n in names]
    claims = [n for n in names if "/" in n and n.split("/")[0] in known]
    # More than one territory ref on the same commit is not a preference to
    # express, it is a question nobody has answered: the two names disagree
    # about who owns this work, and picking whichever git printed first would
    # judge the change against an owner chosen by listing order. The wrong
    # owner approving a change is the failure this map exists to prevent, so
    # ambiguity is raised rather than resolved. (codex, #1636)
    # The PREFIX is what decides the owner, so that is what has to disagree.
    # `t1/a` and `t1/b` on one commit are two names for one territory and no
    # question at all; refusing them called a naming habit a conflict.
    if len({n.split("/")[0] for n in claims}) > 1:
        raise Ambigu(sorted(set(claims)))
    if claims:
        return claims[0]
    for n in names:
        if "/" in n:
            return n
    return names[0] if names else None


def main() -> int:
    if not MAP.exists():
        print(f"SKIPPED (not a pass): {MAP} is missing, so nothing declares who owns what",
              file=sys.stderr)
        return 3

    territories = load()
    if not territories:
        print("SKIPPED (not a pass): the territory map is empty, so this checked nothing",
              file=sys.stderr)
        return 3

    problems: list[str] = []

    # --- 1. the map must not contradict itself ---------------------------
    # Compared as literal patterns rather than by expanding them: two globs that
    # overlap only on files nobody has created yet still overlap, and that is a
    # collision waiting for someone to create the file.
    in_repo = [t for t in territories if "repo" not in t]
    for i, a in enumerate(in_repo):
        for b in in_repo[i + 1:]:
            shared = set(a.get("paden", [])) & set(b.get("paden", []))
            for pattern in sorted(shared):
                if any(fnmatch.fnmatch(pattern, e) for e in a.get("uitgezonderd", [])):
                    continue
                if any(fnmatch.fnmatch(pattern, e) for e in b.get("uitgezonderd", [])):
                    continue
                problems.append(
                    f"{a['id']} and {b['id']} both claim `{pattern}`. "
                    "Two owners for one path is the collision this map exists to prevent."
                )

    # KNOWN LIMIT, worth stating rather than discovering (t3, 01-09-2026): on a
    # shared branch this attributes every changed file to whoever the branch name
    # says, so a relay branch that several terminals resolved reads as one
    # terminal reaching everywhere. Narrowing the base to the pusher's own
    # commits would fix it and is not done here; until then a shared branch needs
    # the per-path review the relay used, not this check alone.

    # --- 2. the branch must have stayed inside its own -------------------
    #
    # Every arm below RECORDS what it found and none of them exits. Three arms
    # each returning their own verdict is how the map half kept getting
    # swallowed: the detached-HEAD arm returned before the shared `if problems`
    # block (fixed once), and the unnamed-branch arm still did -- so a real
    # overlap, found and collected, went unreported behind "this branch names
    # no territory". A `feature/alias` ref pointing at a detached HEAD lands in
    # exactly that arm. One place decides, at the end, on everything collected.
    # (codex, #1636)
    overgeslagen: str | None = None
    checked_files = 0
    unowned = 0
    # Collected, not returned. The first version of this returned 2 right here
    # -- before the shared judgement below -- so a real overlap, found and
    # collected a few lines up, disappeared behind "could not tell whose branch
    # this is". That is the third time in this file that a verdict decided in
    # one arm hid a verdict decided in another, and the second time I wrote it
    # while removing it. Ambiguity is a reason the branch half cannot run; it is
    # not a reason to stop reporting the half that did. (codex, #1636)
    ambigu: Ambigu | None = None
    branch = None
    try:
        branch = current_branch()
    except Ambigu as exc:
        ambigu = exc

    if branch and "/" in branch and branch.split("/")[0] in {t["id"] for t in territories}:
        tid = branch.split("/")[0]
        mine = next(t for t in territories if t["id"] == tid)
        result = changed_files()
        if result is None:
            overgeslagen = ("could not diff against master, so the branch's reach "
                            "is unknown")
        else:
            files, base = result
            checked_files = len(files)
            for path in files:
                # The map itself is deliberately editable from anywhere: taking
                # on work in another territory is a commit, not a silent edit.
                if path == ".claude/territories.toml":
                    continue
                if owns(mine, path):
                    continue
                other = [t["id"] for t in in_repo if owns(t, path)]
                if not other:
                    # Nobody claims it. Allowed -- a complete map of a repository
                    # this size is not maintainable -- but counted, so the map can
                    # grow towards the places that turn out to be contested.
                    unowned += 1
                    continue
                problems.append(
                    f"branch `{branch}` is in {tid} but changed `{path}` "
                    f"(that belongs to {', '.join(other)}). "
                    "Claim it in .claude/territories.toml, or leave it to its owner."
                )
    elif branch is None or branch == "HEAD":
        overgeslagen = ("detached HEAD and no branch name from the environment or "
                        "from a ref pointing here, so whether this work stayed "
                        "inside its own territory was NOT checked. Set "
                        "TERRITORY_BRANCH=<territory>/<what> to check it")
    elif branch != "master":
        # A branch that names no territory used to be skipped, which made opting
        # out free: rename `t1/x` to `upstream/x` and nothing checks you again.
        # All three `upstream/*` branches went unchecked that way, and the moment
        # one was renamed the guard found two real violations in it.
        #
        # So an unnamed branch is skipped only while it stays out of owned
        # ground. The moment it edits a path somebody owns, it has to say who it
        # is -- which is the whole question the map exists to answer.
        result = changed_files()
        if result is None:
            overgeslagen = ("could not diff against master, so an unnamed branch's "
                            "reach is unknown")
        else:
            files, _ = result
            checked_files = len(files)
            # Counted here rather than read back off `problems`: that list also
            # holds the map's own overlaps, so asking it whether THIS arm found
            # anything is one arm reading another's state -- the mistake this
            # whole restructure exists to remove.
            overtreden = 0
            for f in files:
                owners = [t["id"] for t in in_repo if owns(t, f)]
                if owners:
                    overtreden += 1
                    problems.append(
                        f"branch `{branch}` names no territory and changed `{f}` "
                        f"(that belongs to {', '.join(owners)}). Name it "
                        "`<territory>/<what>`, e.g. `t2/ci-guards`, so the map can "
                        "check it -- skipping unnamed branches made opting out free, "
                        "which is how three branches went unchecked.")
            if not overtreden:
                overgeslagen = (f"branch `{branch}` names no territory, but changed no "
                                "owned file either. Name it `<territory>/<what>` if "
                                "that changes")

    # --- one judgement, on everything collected --------------------------
    # An overlap is a real failure and outranks "could not check the rest",
    # so it is tested first and reported whichever arm ran.
    if ambigu is not None:
        print(f"{len(ambigu.namen)} territory refs point at this commit: "
              f"{', '.join(ambigu.namen)}. They name different territories, so "
              "they disagree about who owns this work, and choosing between them "
              "by listing order would judge it against an owner nobody named. "
              "Set TERRITORY_BRANCH to the one that applies.", file=sys.stderr)

    if problems:
        print(f"Territories: {len(problems)} problem(s)\n", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        if overgeslagen:
            print(f"\n  (and the map WAS checked, but {overgeslagen})", file=sys.stderr)
        return 1
    if ambigu is not None:
        return 2
    if overgeslagen:
        print(f"SKIPPED (not a pass): {overgeslagen}. The map itself was checked "
              f"for overlaps and had none.", file=sys.stderr)
        return 3

    print(f"\u2713 {len(territories)} territories, no overlap"
          + (f"; branch stayed inside its own across {checked_files} changed file(s)"
             + (f" ({unowned} unclaimed)" if unowned else "")
             if checked_files else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
