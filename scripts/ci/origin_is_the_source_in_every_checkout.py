#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`origin` is the source here, and this repository has one checkout on a machine.

WHY A GUARD AND NOT A NOTE

Both halves have already produced wrong answers rather than untidiness.

`origin` pointed at the GitLab backup in this checkout while GitHub had been the
source since 25-08-2026. Every guard that compares against "origin" was then
measuring the wrong repository: `territories_do_not_overlap.py` reported 62 files
as a branch's own work when none of them were, and `mr_staleness.py` counted how
far a branch was behind a mirror that ran 348 commits behind master (#231, #291).
Neither failed loudly. Both answered confidently about the wrong ref.

The second half is how the first one arrives: a repository with two checkouts on
one disk has two answers to every question, and the one you are standing in is
not necessarily the one that gets pushed. That is the shape of the `lopdf`
upgrade that sat finished and stranded in a second copy, and of the two
unrelated histories now sharing one remote in `pdfluent-playground`.

WHAT IT CHECKS

  * `origin` exists here and resolves to the SOURCE repository of the topology
    table in CLAUDE.md -- not the backup, not something else
  * no remote here carries the backup's URL under any name but the one the table
    gives it, so `git push` cannot reach the copy by a name that reads primary
  * this repository has at most one checkout under the scanned roots

  and, as a warning rather than a refusal, a second checkout of any OTHER
  repository the table names.

WHY THE OTHERS ONLY WARN

A landing on this repository should not be closed over a duplicate checkout of
the website, which is a fact about another repository that the person landing
cannot fix from here and that no commit can change. This repository's own
duplicates are the ones a landing can and should be stopped for -- the copy that
answers differently is the one you might have been standing in. Guards that
refuse over something outside the change closed master four times in one day
(05-09-2026), and this is deliberately not the fifth.

THE ROOTS

Anchored at the parent of the MAIN working tree, not of the current one, so every
worktree of this repository gets the same answer instead of one per directory it
happens to sit in. `CHECKOUT_ROOTS` (colon-separated) overrides it; the tests use
that, and so can a machine whose checkouts live somewhere else.

Exit codes:
  0  origin is the source here and this repository has one checkout
  1  it does not, or it has more than one, or the table could not be read
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from the_topology_agrees_with_the_mirror_gate import (  # noqa: E402
    gate_remotes, rows)

REPO = pathlib.Path(__file__).resolve().parents[2]

# How deep to look under a root. Two is enough for `~/Documents/XFA` and for
# `~/Documents/PDFluent/pdfluent`, which is one of the checkouts this exists to
# find; three leaves room without walking a whole home directory.
MAX_DEPTH = 3

# Not descended into. `.git` and every dotted directory because a worktree of
# this repository lives under `.worktrees/`, and the rest because they are large,
# and a vendored dependency's own `.git` is not a checkout of anything of ours.
PRUNED = {".git", "node_modules", "target", "dist", "build", "vendor",
          "Library", "Applications"}


def clean_environment() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    A pre-push hook exports GIT_DIR, a subprocess inherits it, and a `git config
    --get` meant for another directory then answers about the repository the hook
    is running in. That put `core.bare = true` on this one and stopped thirty
    worktrees -- damage outside the script, not a wrong answer inside it.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str, cwd: pathlib.Path | None = None) -> str | None:
    try:
        r = subprocess.run(["git", *args], capture_output=True, text=True,
                           check=False, timeout=60, cwd=cwd,
                           env=clean_environment())
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def identity(url: str) -> str:
    """`git@github.com:a/b.git` and `https://github.com/A/B/` as one string.

    Host and path only. A repository is the same repository whether it is
    reached over ssh or https, and the table writes it a third way again --
    without a scheme at all.
    """
    u = url.strip().strip("`").strip()
    u = re.sub(r"^[A-Za-z][A-Za-z0-9+.-]*://", "", u)
    u = re.sub(r"^[^@/]+@", "", u)
    u = u.replace(":", "/", 1)
    u = re.sub(r"\.git/*$", "", u)
    return u.rstrip("/").lower()


def remotes(at: pathlib.Path) -> dict[str, str]:
    """Every remote of the checkout at `at`, as {name: url}."""
    out = git("remote", "-v", cwd=at)
    if out is None:
        return {}
    found: dict[str, str] = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 2:
            found.setdefault(parts[0], parts[1])
    return found


def main_worktree() -> pathlib.Path:
    """The checkout that owns the object store, seen from any worktree of it."""
    common = git("rev-parse", "--path-format=absolute", "--git-common-dir",
                 cwd=REPO)
    if not common:
        return REPO
    path = pathlib.Path(common)
    return path.parent if path.name == ".git" else REPO


def roots() -> list[pathlib.Path]:
    override = os.environ.get("CHECKOUT_ROOTS", "").strip()
    if override:
        return [pathlib.Path(p).expanduser() for p in override.split(":") if p]
    return [main_worktree().parent]


def checkouts() -> list[pathlib.Path]:
    """Directories under the roots that are a clone -- a `.git` DIRECTORY.

    A `.git` FILE is a worktree, which shares the clone's remotes and its object
    store. Counting those as separate checkouts would report this repository's
    own thirty working directories as the drift the guard is looking for.
    """
    found: list[pathlib.Path] = []
    for root in roots():
        if not root.is_dir():
            continue
        base = len(root.resolve().parts)
        for here, subdirs, _ in os.walk(root, topdown=True):
            path = pathlib.Path(here)
            depth = len(path.resolve().parts) - base
            if depth >= MAX_DEPTH:
                subdirs[:] = []
            else:
                subdirs[:] = [d for d in subdirs
                              if d not in PRUNED and not d.startswith(".")]
            if (path / ".git").is_dir():
                found.append(path)
    return sorted(set(found))


def table_repositories() -> dict[str, tuple[str, str]]:
    """{identity: (repository as written, remote name)} for every table row."""
    known: dict[str, tuple[str, str]] = {}
    for repository, remote, _role in rows():
        key = identity(repository)
        if key:
            known.setdefault(key, (repository, remote))
    return known


def main() -> int:
    try:
        source_remote, mirror_remote = gate_remotes()
    except SystemExit as stop:
        print(f"origin_is_the_source_in_every_checkout: {stop}", file=sys.stderr)
        return 1

    table = rows()
    named = {remote: repository for repository, remote, _ in table}
    source = identity(named.get(source_remote, ""))
    mirror = identity(named.get(mirror_remote, ""))
    if not source or not mirror:
        print("[checkouts] SKIPPED (not a pass): the topology table in CLAUDE.md "
              f"does not give both '{source_remote}' and '{mirror_remote}' a "
              "repository, so there is nothing to compare a remote against. "
              "the_topology_agrees_with_the_mirror_gate.py says which half is "
              "missing; nothing here was checked.", file=sys.stderr)
        return 1

    complaints: list[str] = []
    warnings: list[str] = []

    here = remotes(REPO)
    if "origin" not in here:
        complaints.append(
            "this checkout has no remote called 'origin'. Every guard that falls "
            f"back to `origin/master` -- and there are a dozen -- then has no base "
            f"to compare against. Point it at {named[source_remote]}")
    elif identity(here["origin"]) != source:
        points_at = "the BACKUP" if identity(here["origin"]) == mirror else "neither"
        complaints.append(
            f"'origin' points at {here['origin']} ({points_at} of the two "
            f"repositories in the table), and the source is {named[source_remote]}. "
            "A bare `git push` then goes to the copy, and every guard that reads "
            "`origin/master` measures against it -- which is how 62 files were "
            "reported as a branch's own work when none of them were (#291)")

    for name, url in sorted(here.items()):
        if identity(url) == mirror and name != mirror_remote:
            complaints.append(
                f"remote '{name}' carries the backup {named[mirror_remote]}, and "
                f"the table calls that remote '{mirror_remote}'. One name per role: "
                "a backup reachable under a second name is a backup somebody "
                "pushes to by accident")

    known = table_repositories()
    walked = checkouts()
    seen: dict[str, list[pathlib.Path]] = {}
    for path in walked:
        urls = {identity(u) for u in remotes(path).values()}
        for key in urls & set(known):
            seen.setdefault(key, []).append(path)

    # FLOOR: the walk has to find the checkout the guard is standing in. Not
    # "find it under the source's identity" -- a checkout whose origin is wrong
    # is exactly the case above, and a floor that swallowed it would report a
    # broken search instead of the misnamed remote. This is about the walk.
    scanned = roots()
    mine = main_worktree()
    if any(mine == r or r in mine.parents for r in scanned) and mine not in walked:
        print(f"[checkouts] SKIPPED (not a pass): the scan of "
              f"{', '.join(str(r) for r in scanned)} did not find "
              f"{mine}, the checkout it is standing in. The search is broken, so "
              "the count below would approve anything.", file=sys.stderr)
        return 1

    for key, paths in sorted(seen.items()):
        if len(paths) < 2:
            continue
        repository, _ = known[key]
        where = "\n      ".join(str(p) for p in paths)
        if key == source:
            complaints.append(
                f"{repository} has {len(paths)} checkouts on this machine:\n"
                f"      {where}\n"
                "    Two checkouts are two answers to every question, and the one "
                "you are standing in is not necessarily the one that gets pushed. "
                "Keep one; confirm the others hold no unpushed commits first")
        else:
            warnings.append(
                f"{repository} has {len(paths)} checkouts here: {', '.join(str(p) for p in paths)}")

    for line in warnings:
        print(f"[checkouts] WARNING: {line}", file=sys.stderr)
    if warnings:
        print("[checkouts] Those are other repositories: reported, not refused. A "
              "landing here cannot fix them and no commit here changes them.",
              file=sys.stderr)

    if complaints:
        print(f"\n[checkouts] FATAL: {len(complaints)} problem(s) with where the "
              "code lives:", file=sys.stderr)
        for c in complaints:
            print(f"  - {c}", file=sys.stderr)
        print("\n`origin` means the primary remote, in every checkout, and a "
              "repository has one checkout on a machine. Both are states somebody "
              "can drift back into by hand, which is why they are checked rather "
              "than written down (#291).", file=sys.stderr)
        return 1

    print(f"[checkouts] OK: 'origin' is {named[source_remote]}; "
          f"{len(seen.get(source, []))} checkout(s) of it under "
          f"{', '.join(str(r) for r in scanned)}"
          + (f"; {len(warnings)} other repository/repositories duplicated" if warnings else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
