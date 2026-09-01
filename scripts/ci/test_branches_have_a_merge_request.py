#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Tests for branches_have_a_merge_request.py.

Built against scratch repositories, so the cases are about the counting and the
skip -- not about whatever branch happens to be checked out.

The one that matters is the skip: without `gh` the guard cannot know whether a
branch has a merge request, and a guard that silently passes in that situation
is indistinguishable from one that checked. That is exactly the shape #272 is
about, so this file refuses to let its own guard have it.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

def schone_omgeving() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed."""
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a
    # fixture READING the real repository, not WRITING to the real config.
    # A fixture's `git config user.email t@t` reached a real worktree that
    # way and stamped a test identity onto every later rebase there. (#297)
    return sealed_env()


BEWAKER = pathlib.Path(__file__).with_name("branches_have_a_merge_request.py")


def git(map_: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=map_, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=schone_omgeving())


def bouw(map_: pathlib.Path, commits: int) -> None:
    git(map_, "init", "-q", "-b", "master")
    git(map_, "config", "user.email", "t@t")
    git(map_, "config", "user.name", "t")
    (map_ / "a.txt").write_text("0")
    git(map_, "add", "a.txt")
    git(map_, "commit", "-qm", "start")
    # A remote ref the guard can compare against, without a real remote.
    git(map_, "update-ref", "refs/remotes/origin/master", "HEAD")
    git(map_, "checkout", "-qb", "werk")
    for n in range(commits):
        (map_ / "a.txt").write_text(str(n + 1))
        git(map_, "add", "a.txt")
        git(map_, "commit", "-qm", f"c{n}")


def draai(map_: pathlib.Path, met_gh: bool) -> subprocess.CompletedProcess[str]:
    omgeving = dict(os.environ)
    if not met_gh:
        # A PATH with git on it and gh not. Emptying PATH altogether hides git
        # too, and then the guard reports a detached HEAD and skips for a
        # completely different reason -- which the first version of this test
        # happily accepted.
        bin_ = map_ / "bin"
        bin_.mkdir(exist_ok=True)
        echte_git = shutil.which("git")
        if echte_git is None:
            print("SKIPPED (not a pass): no git on PATH to link.", file=sys.stderr)
            raise SystemExit(0)
        doel = bin_ / "git"
        if not doel.exists():
            doel.symlink_to(echte_git)
        omgeving["PATH"] = str(bin_)
    return subprocess.run([sys.executable, str(BEWAKER)], cwd=map_,
                          capture_output=True, text=True, env=omgeving, check=False)


def main() -> int:
    fouten: list[str] = []

    # 1. A branch a few commits ahead is normal work; say so and pass.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, 3)
        r = draai(m, met_gh=True)
        if r.returncode != 0:
            fouten.append(f"a 3-commit branch fails: {r.stdout}{r.stderr}")
        if "3 commit" not in r.stdout:
            fouten.append(f"the count is not reported: {r.stdout}")

    # 2. Far ahead, and `gh` unavailable: it must announce the skip, not pass
    #    quietly. Without this the guard is silent exactly when it matters.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, 60)
        r = draai(m, met_gh=False)
        # Not just any skip: it must be the one about `gh`. The first version of
        # this case accepted "no master/main remote ref" too, and then a guard
        # that returned 0 silently when gh was missing passed it. A test that
        # settles for the wrong skip is the thing this whole guard is about.
        if "`gh` is not" not in r.stderr:
            fouten.append(
                "60 commits ahead with no gh, and the gh-skip is not announced: "
                f"rc={r.returncode} stdout={r.stdout!r} stderr={r.stderr!r}"
            )

    # 3. On master there is nothing to ask.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, 1)
        git(m, "checkout", "-q", "master")
        r = draai(m, met_gh=True)
        if r.returncode != 0 or "nothing to ask" not in r.stdout:
            fouten.append(f"master is not recognised: {r.stdout}{r.stderr}")

    if fouten:
        print("[test-branch-mr] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-branch-mr] OK: the count, the announced skip, and master itself.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
