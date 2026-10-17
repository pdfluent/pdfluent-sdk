#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The tracked-symlink guard bites on a link and passes a clean tree (#636).

Each case builds a throwaway git repository -- `git init`, `git add`, no
commit needed, since the guard reads the index -- and runs the guard against
it with `--root`, so nothing here touches the real tree. Both directions
matter: a guard that never goes red is decoration.

Exit codes:
  0  every case passed
  1  a case failed
  2  git is not available (announced, never silent)
"""
from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from fixture_env import sealed_env  # noqa: E402

GUARD = HERE / "no_tracked_symlinks.py"


def git(*args: str, cwd: pathlib.Path) -> None:
    subprocess.run(
        ["git", *args],
        cwd=cwd,
        env=sealed_env(cwd=cwd),
        capture_output=True,
        text=True,
        check=True,
    )


def fresh_repo() -> pathlib.Path:
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="nolinks-"))
    git("init", "-q", cwd=tmp)
    git("config", "user.email", "guard@test.invalid", cwd=tmp)
    git("config", "user.name", "guard", cwd=tmp)
    git("config", "commit.gpgsign", "false", cwd=tmp)
    return tmp


def run_guard(root: pathlib.Path) -> int:
    # Sealed too: the guard shells out to git, and an inherited GIT_DIR from
    # a hook would point that call at the real repository instead of the
    # fixture's.
    return subprocess.run(
        [sys.executable, str(GUARD), "--root", str(root)],
        env=sealed_env(),
        capture_output=True,
        text=True,
    ).returncode


def main() -> int:
    if shutil.which("git") is None:
        print("FATAL: git is not on PATH; the guard cannot be exercised", file=sys.stderr)
        return 2
    failures: list[str] = []

    # 1. A tracked symlink outside the allow-list fails.
    root = fresh_repo()
    (root / "target.pdf").write_bytes(b"%PDF-1.4estore\n")
    os.symlink("target.pdf", root / "linked.pdf")
    git("add", "target.pdf", "linked.pdf", cwd=root)
    if run_guard(root) != 1:
        failures.append("a tracked symlink must fail the guard")
    shutil.rmtree(root, ignore_errors=True)

    # 2. A tree with no symlinks at all passes.
    root = fresh_repo()
    (root / "plain.pdf").write_bytes(b"%PDF-1.4estore\n")
    git("add", "plain.pdf", cwd=root)
    if run_guard(root) != 0:
        failures.append("a tree without symlinks must pass")
    shutil.rmtree(root, ignore_errors=True)

    # 3. The allow-listed hook passes as a link.
    root = fresh_repo()
    (root / ".githooks").mkdir()
    (root / "hook-impl").write_text("#!/bin/sh\n")
    os.symlink("../hook-impl", root / ".githooks" / "commit-msg")
    git("add", "hook-impl", ".githooks/commit-msg", cwd=root)
    if run_guard(root) != 0:
        failures.append("the allow-listed .githooks/commit-msg link must pass")
    shutil.rmtree(root, ignore_errors=True)

    # 4. The guard reads the index, not the filesystem: an untracked link --
    # the kind node_modules and target/ are full of -- must not fail it.
    root = fresh_repo()
    (root / "target.pdf").write_bytes(b"%PDF-1.4estore\n")
    os.symlink("target.pdf", root / "loose.pdf")
    git("add", "target.pdf", cwd=root)
    if run_guard(root) != 0:
        failures.append("an untracked symlink must not fail the guard")
    shutil.rmtree(root, ignore_errors=True)

    # 5. The allow-list excuses one path, not the tree: a second link fails.
    root = fresh_repo()
    (root / ".githooks").mkdir()
    (root / "hook-impl").write_text("#!/bin/sh\n")
    (root / "target.pdf").write_bytes(b"%PDF-1.4estore\n")
    os.symlink("../hook-impl", root / ".githooks" / "commit-msg")
    os.symlink("target.pdf", root / "linked.pdf")
    git("add", "hook-impl", ".githooks/commit-msg", "target.pdf", "linked.pdf", cwd=root)
    if run_guard(root) != 1:
        failures.append("a link beside the allow-listed one must still fail")
    shutil.rmtree(root, ignore_errors=True)

    for f in failures:
        print("FAIL", f)
    if failures:
        return 1
    print("symlink guard: a tracked link fails, a clean tree and the allow-list pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
