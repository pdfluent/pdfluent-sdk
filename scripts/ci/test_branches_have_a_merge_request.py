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

The other one that matters is the pre-push ref list. The guard promised "the
next push fails" while reading a tracking ref that a renaming refspec never
creates, so the promise could not come true (codex, #1610). Cases 4 to 6 feed
the guard the lines git hands a pre-push hook and require it to act on them:
fail when the remote has the branch and no request exists, ask `gh` for the
remote's name and not the local one, and stay quiet only while the remote sha
is still all zeros.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env, inside_the_sandbox

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

def schone_omgeving(cwd=None) -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed."""
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a
    # fixture READING the real repository, not WRITING to the real config.
    # A fixture's `git config user.email t@t` reached a real worktree that
    # way and stamped a test identity onto every later rebase there. (#297)
    return sealed_env(cwd=cwd)


BEWAKER = pathlib.Path(__file__).with_name("branches_have_a_merge_request.py")


def git(map_: pathlib.Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=map_, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=schone_omgeving(cwd=map_))


def bouw(map_: pathlib.Path, commits: int) -> None:
    git(map_, "init", "-q", "-b", "master")
    git(map_, "config", "user.email", "t@t")
    git(map_, "config", "user.name", "t")
    (map_ / "a.txt").write_text("0")
    git(map_, "add", "a.txt")
    git(map_, "commit", "-qm", "start")
    # A remote ref the guard can compare against, without a real remote.
    git(map_, "update-ref", "refs/remotes/origin/master", "HEAD")
    # The branch's commits through one fast-import rather than one `git commit`
    # each: the fatal threshold is 150 commits, and at half a second a commit
    # the cases below took minutes.
    stroom: list[str] = []
    for n in range(commits):
        boodschap = f"c{n}\n"
        inhoud = str(n + 1)
        stroom.append(
            "commit refs/heads/werk\n"
            f"committer t <t@t> {1_700_000_000 + n} +0000\n"
            f"data {len(boodschap)}\n{boodschap}"
            + ("from refs/heads/master\n" if n == 0 else "")
            + f"M 100644 inline a.txt\ndata {len(inhoud)}\n{inhoud}\n\n"
        )
    if commits:
        subprocess.run(["git", "fast-import", "--quiet"], cwd=map_, check=True,
                       input="".join(stroom), text=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       env=schone_omgeving(cwd=map_))
        git(map_, "checkout", "-q", "werk")
    else:
        git(map_, "checkout", "-qb", "werk")


NUL = "0" * 40
EEN = "1" * 40


def draai(map_: pathlib.Path, met_gh: bool | str,
          stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    """Run the guard. `met_gh`: True for the real PATH, False for git only,
    "leeg" for git plus a fake `gh` that answers with no pull requests and logs
    its arguments to GH_LOG. `stdin` is what a pre-push hook would receive."""
    omgeving = dict(os.environ)
    if met_gh == "leeg":
        bin_ = map_ / "bin"
        bin_.mkdir(exist_ok=True)
        echte_git = shutil.which("git")
        if echte_git is None:
            print("SKIPPED (not a pass): no git on PATH to link.", file=sys.stderr)
            raise SystemExit(0)
        doel = bin_ / "git"
        if not doel.exists():
            doel.symlink_to(echte_git)
        nep = bin_ / "gh"
        nep.write_text('#!/bin/sh\necho "$@" >> "$GH_LOG"\necho "[]"\n')
        nep.chmod(0o755)
        omgeving["PATH"] = str(bin_)
        omgeving["GH_LOG"] = str(map_ / "gh.log")
    elif not met_gh:
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
                          capture_output=True, text=True, env=omgeving, check=False,
                          input=stdin if stdin is not None else "")


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

    # 4. The pre-push ref list says the remote HAS this branch, under another
    #    name, and `gh` finds no request: fatal. Before the ref list was read,
    #    the guard looked for github/werk, found nothing, and said "push it,
    #    then open one" on every push -- the promise this case pins down.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, 160)
        r = draai(m, met_gh="leeg",
                  stdin=f"refs/heads/werk {EEN} refs/heads/elders {EEN}\n")
        if r.returncode != 1 or "no merge request" not in r.stderr:
            fouten.append(
                "160 ahead, on the remote per the pre-push ref list, no request, "
                f"and not fatal: rc={r.returncode} stdout={r.stdout!r} "
                f"stderr={r.stderr!r}"
            )
        # 5. And it asked `gh` about the REMOTE's name. A renaming refspec means
        #    the local name is not what a pull request would be opened from.
        log = (m / "gh.log").read_text() if (m / "gh.log").exists() else ""
        if "--head elders" not in log:
            fouten.append(f"gh was not asked about the remote branch name: {log!r}")

    # 6. Remote sha all zeros: the remote has never seen the branch, so there
    #    is nothing to demand yet. Quiet, and it says where it read that.
    with tempfile.TemporaryDirectory() as d:
        m = pathlib.Path(d)
        bouw(m, 160)
        r = draai(m, met_gh="leeg",
                  stdin=f"refs/heads/werk {EEN} refs/heads/werk {NUL}\n")
        if r.returncode != 0 or "does not exist on the remote yet" not in r.stdout:
            fouten.append(
                "160 ahead, not on the remote per the ref list, and not the quiet "
                f"branch: rc={r.returncode} stdout={r.stdout!r} stderr={r.stderr!r}"
            )

    if fouten:
        print("[test-branch-mr] FATAL:", file=sys.stderr)
        for f in fouten:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("[test-branch-mr] OK: the count, the announced skip, master itself, "
          "and the pre-push ref list.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
