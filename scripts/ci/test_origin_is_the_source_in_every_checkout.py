#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Tests for origin_is_the_source_in_every_checkout.py, on throwaway checkouts.

Running the guard here proves only that this machine currently agrees with the
table. Each case below builds a repository -- with the topology table and the
mirror gate copied in beside it, because the guard reads both -- puts one thing
wrong, and checks the guard refuses and says which thing.

The six shapes are the ones #291 measured or reasoned about:

  origin points at the backup           the state of this checkout until 05-09-2026
  no origin at all                      every `origin/master` fallback loses its base
  the backup under a second name        a copy reachable by a name that reads primary
  a second checkout of this repository  two answers to every question
  a second checkout of another one      reported, never refused
  a worktree beside the checkout        NOT a second checkout, and there are 190

The last one is the case that decides whether the guard is usable at all: a
worktree shares the clone's remotes and its object store, and counting those
would report this repository's own working directories as drift.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = pathlib.Path(__file__).with_name("origin_is_the_source_in_every_checkout.py")
TOPOLOGY_GUARD = pathlib.Path(__file__).with_name(
    "the_topology_agrees_with_the_mirror_gate.py")
GATE = pathlib.Path(__file__).with_name("mirror_has_not_drifted.py")
TABLE = REPO / "CLAUDE.md"

SOURCE = "https://github.com/pdfluent/engine.git"
BACKUP = "https://github.com/pdfluent/PDFluent-project.git"
OTHER = "https://github.com/pdfluent/pdfluent-playground.git"


def git(*args: str, cwd: pathlib.Path) -> None:
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True,
                       check=False, env=sealed_env(identity=True, cwd=cwd))
    if r.returncode != 0:
        raise SystemExit(f"fixture: `git {' '.join(args)}` failed in {cwd}: "
                         f"{r.stderr.strip()}")


def clone(at: pathlib.Path, **remotes: str) -> pathlib.Path:
    """A repository with the given remotes and nothing in it."""
    at.mkdir(parents=True, exist_ok=True)
    git("init", "--quiet", cwd=at)
    for name, url in remotes.items():
        git("remote", "add", name, url, cwd=at)
    return at


def sandbox(root: pathlib.Path, **remotes: str) -> pathlib.Path:
    """The layout the guard resolves: repo/scripts/ci/<guard> and repo/CLAUDE.md."""
    repo = root / "engine"
    ci = repo / "scripts" / "ci"
    ci.mkdir(parents=True)
    for f in (GUARD, TOPOLOGY_GUARD, GATE):
        shutil.copy(f, ci / f.name)
    shutil.copy(TABLE, repo / "CLAUDE.md")
    clone(repo, **(remotes or {"origin": SOURCE, "gitlab": BACKUP}))
    return repo


def run(repo: pathlib.Path, root: pathlib.Path) -> subprocess.CompletedProcess[str]:
    env = sealed_env(cwd=repo)
    env["CHECKOUT_ROOTS"] = str(root)
    return subprocess.run(
        [sys.executable, str(repo / "scripts" / "ci" / GUARD.name)],
        capture_output=True, text=True, check=False, env=env)


def case(name: str, build, expect_fail: bool, must_say: str = "") -> list[str]:
    with tempfile.TemporaryDirectory() as d:
        root = pathlib.Path(d)
        repo = build(root)
        r = run(repo, root)
        out = r.stdout + r.stderr
        if expect_fail and r.returncode == 0:
            return [f"{name}: accepted (exit 0) -- {out.strip()[:300]}"]
        if not expect_fail and r.returncode != 0:
            return [f"{name}: refused a checkout that is right -- {out.strip()[:300]}"]
        if must_say and must_say.lower() not in out.lower():
            return [f"{name}: does not say why ('{must_say}' absent) -- {out.strip()[:300]}"]
    return []


def right(root: pathlib.Path) -> pathlib.Path:
    return sandbox(root)


def origin_is_the_backup(root: pathlib.Path) -> pathlib.Path:
    return sandbox(root, origin=BACKUP, github=SOURCE)


def no_origin(root: pathlib.Path) -> pathlib.Path:
    return sandbox(root, github=SOURCE, gitlab=BACKUP)


def backup_under_a_second_name(root: pathlib.Path) -> pathlib.Path:
    return sandbox(root, origin=SOURCE, backup=BACKUP)


def a_second_checkout(root: pathlib.Path) -> pathlib.Path:
    repo = sandbox(root)
    clone(root / "engine-again", origin=SOURCE)
    return repo


def a_second_checkout_of_another_repository(root: pathlib.Path) -> pathlib.Path:
    repo = sandbox(root)
    clone(root / "site-one", origin=OTHER)
    clone(root / "site-two", origin=OTHER)
    return repo


def a_worktree_is_not_a_checkout(root: pathlib.Path) -> pathlib.Path:
    repo = sandbox(root)
    # A worktree needs a commit to point at; an empty repository has no HEAD.
    (repo / "README").write_text("x")
    git("add", "README", cwd=repo)
    git("commit", "--quiet", "--no-verify", "-m", "one", cwd=repo)
    git("worktree", "add", "--quiet", "-b", "side", str(root / "beside"), cwd=repo)
    return repo


def main() -> int:
    failures: list[str] = []
    failures += case("origin points at the backup", origin_is_the_backup, True,
                     "BACKUP")
    failures += case("no origin at all", no_origin, True, "no remote called 'origin'")
    failures += case("the backup under a second name", backup_under_a_second_name,
                     True, "one name per role")
    failures += case("a second checkout of this repository", a_second_checkout,
                     True, "checkouts on this machine")
    failures += case("a second checkout of another repository",
                     a_second_checkout_of_another_repository, False, "WARNING")
    failures += case("a worktree is not a second checkout",
                     a_worktree_is_not_a_checkout, False)
    failures += case("origin is the source and stands alone", right, False)

    if failures:
        print("[test-checkouts] FATAL:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-checkouts] OK: a backup as origin, a missing origin, a backup "
          "under a second name and a second checkout are all refused; another "
          "repository's duplicate warns; a worktree is not a checkout.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
