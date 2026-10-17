#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Every worktree of this clone runs the hooks that are IN it.

WHAT HAPPENED

`the_commit_msg_hook_is_wired.py` asks whether `core.hooksPath` is set, and it
asks it once, of the checkout it happens to run in. That is one question short
of the truth, because git has per-worktree config: with
`extensions.worktreeConfig` on -- which it is here -- a worktree carries its own
`config.worktree`, and a `core.hooksPath` set there beats the shared one.

Measured on 10-09-2026 across the 72 worktrees of this clone: 70 pointed at the
tracked `.githooks`, and two pointed at `<clone>/.git/hooks`, a directory that
holds nothing but git's own `.sample` files. In those two, `git commit` ran no
commit-msg hook at all: not the identity check, not the sign-off, not the
attribution guard. Nothing said so. A hook that is not installed fails by being
quiet, and this repository has already paid for that once -- four months of
commits under a rule that enforced nothing (#1660).

Both offenders were made by a tool that creates worktrees of its own, so this
is not a thing somebody did once. It comes back every time such a worktree is
made, and the commits that come out of it look exactly like the others.

WHAT IS CHECKED

For every worktree of this clone that still exists on disk: the effective
`core.hooksPath` must resolve to that worktree's own tracked `.githooks`.

Its own, and not another worktree's: the hooks that judge a commit have to be
the hooks in the tree the commit is made from, or a branch that changes a hook
is judged by the version on somebody else's branch.

WHAT IS NOT CHECKED HERE

A clone where NO worktree has it set at all. That is a fresh checkout where
nobody ran the installer -- a CI checkout is one, on every run -- and it is
`the_commit_msg_hook_is_wired.py` layer 3 that owns that question, on the
machine where a commit is actually written. Reporting it here as well would
make this guard red on every pull request for a reason that has nothing to do
with the pull request, and a red that means nothing teaches people to stop
reading reds.

Exit codes:
    0  every worktree runs its own tracked hooks, or no worktree has hooks
       installed at all and this is not the guard that judges that
    1  at least one worktree runs different hooks, or none, while others run
       the tracked ones
    2  the worktrees cannot be listed, which is itself a broken clone
"""
from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
GIT = "git"


def sealed() -> dict[str, str]:
    """`fixture_env.sealed_env()`: no `GIT_*` from the caller, config sealed.

    This guard only READS config, so the seal changes nothing about its answer.
    It goes through the shared helper because a second way of building a git
    environment is how the two drift apart.
    """
    spec = importlib.util.spec_from_file_location(
        "fixture_env", pathlib.Path(__file__).resolve().parent / "fixture_env.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.sealed_env()


def worktrees(env: dict[str, str]) -> list[pathlib.Path]:
    """The worktrees of this clone, main one included, that exist on disk.

    A worktree whose directory is gone is a stale administrative entry (`git
    worktree prune` removes it) and has no config to judge: it cannot run a
    hook because there is nothing to commit in.
    """
    done = subprocess.run([GIT, "worktree", "list", "--porcelain"],
                          cwd=REPO, capture_output=True, text=True,
                          env=env, check=False)
    if done.returncode != 0:
        raise OSError(done.stderr.strip() or "git worktree list failed")
    paths = []
    for line in done.stdout.splitlines():
        if line.startswith("worktree "):
            path = pathlib.Path(line[len("worktree "):])
            if path.is_dir():
                paths.append(path)
    return paths


def hooks_path(worktree: pathlib.Path, env: dict[str, str]) -> str:
    """What git will use as the hooks directory in this worktree.

    Read with `git -C <worktree>`, which is the only way to get the per-worktree
    answer: `config.worktree` belongs to the checkout, and reading the shared
    file directly -- the obvious shortcut -- gives the answer for a different
    worktree than the one being asked about. That shortcut is the whole defect.
    """
    done = subprocess.run([GIT, "-C", str(worktree), "config", "--get", "core.hooksPath"],
                          capture_output=True, text=True, env=env, check=False)
    return done.stdout.strip()


def resolves_to_own_hooks(worktree: pathlib.Path, value: str) -> bool:
    """Does that value name this worktree's own `.githooks`?

    git resolves a relative `core.hooksPath` against the top of the working
    tree, so `.githooks` and `<worktree>/.githooks` are the same directory and
    both are right. Compared as resolved paths rather than as strings, because
    `/private/tmp` and `/tmp` are the same directory on macOS and a string
    comparison would call one of them a finding.
    """
    if not value:
        return False
    candidate = pathlib.Path(value)
    if not candidate.is_absolute():
        candidate = worktree / candidate
    try:
        return candidate.resolve() == (worktree / ".githooks").resolve()
    except OSError:
        return False


def main() -> int:
    env = sealed()
    try:
        trees = worktrees(env)
    except OSError as error:
        print(f"[worktree-hooks] the worktrees of this clone cannot be listed: {error}",
              file=sys.stderr)
        return 2

    correct: list[pathlib.Path] = []
    wrong: list[tuple[pathlib.Path, str]] = []
    unset: list[pathlib.Path] = []
    for tree in trees:
        value = hooks_path(tree, env)
        if resolves_to_own_hooks(tree, value):
            correct.append(tree)
        elif value:
            wrong.append((tree, value))
        else:
            unset.append(tree)

    if not correct and not wrong:
        # Nobody installed the hooks anywhere. A CI checkout, and not this
        # guard's question -- see the header.
        print(f"[worktree-hooks] NOT JUDGED here: none of the {len(trees)} "
              "worktree(s) of this clone has core.hooksPath set, which is what a "
              "fresh checkout looks like. Whether THIS clone should have it is "
              "layer 3 of scripts/ci/the_commit_msg_hook_is_wired.py, judged by "
              "the local gate on a machine where commits are written.")
        return 0

    if not wrong and not unset:
        print(f"[worktree-hooks] OK: all {len(correct)} worktree(s) run their own "
              "tracked .githooks")
        return 0

    print("[worktree-hooks] a worktree of this clone runs hooks that are not its "
          "own, so a commit made there is judged by nothing:\n", file=sys.stderr)
    for tree, value in wrong:
        print(f"  - {tree}\n      core.hooksPath = {value}", file=sys.stderr)
    for tree in unset:
        print(f"  - {tree}\n      core.hooksPath is unset while "
              f"{len(correct)} other worktree(s) of this clone have it",
              file=sys.stderr)
    print("\n  Fix each of them, in the worktree itself:\n\n"
          "    git -C <worktree> config --worktree core.hooksPath .githooks\n\n"
          "  `--worktree` because this clone has extensions.worktreeConfig on: a "
          "value set there is the one git uses, and setting the shared one leaves "
          "the override in place. `git -C <worktree> config --unset --worktree "
          "core.hooksPath` also works when the shared value is already right.\n\n"
          "  It cannot be set for you from inside the repository: it is local "
          "config, which is exactly why it has to be checked.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
