#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the worktree-hooks guard says about a clone with real worktrees.

Every case builds an actual repository with actual `git worktree add`, because
the defect the guard exists for lives in per-worktree config: with
`extensions.worktreeConfig` on, a value in `config.worktree` beats the shared
one, and a fixture that only wrote `.git/config` would never reproduce the
state that was measured on this clone -- two worktrees pointing at
`<clone>/.git/hooks` while the shared config said `.githooks`.

Four cases, and the last two are the ones that make the first two mean
something: the guard must be SILENT on a fresh checkout (or it is red on every
pull request and gets ignored) and it must still be called by the local gate
(or it is a file nobody runs).
"""
from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))
from fixture_env import gate_aanroepen, sealed_env, wegwerp_map  # noqa: E402

GUARD = "scripts/ci/every_worktree_runs_the_tracked_hooks.py"
GIT = "git"

ran = 0
fails: list[str] = []


def expect(what: str, ok: bool, detail: str = "") -> None:
    global ran
    ran += 1
    print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + (f" -- {detail}" if not ok and detail else ""))
    if not ok:
        fails.append(what)


def build(worktree_config: bool = True):
    """A clone carrying the guard, a `.githooks/`, one commit and one worktree."""
    root = pathlib.Path(wegwerp_map()) / "repo"
    (root / "scripts" / "ci").mkdir(parents=True)
    (root / ".githooks").mkdir()
    (root / ".githooks" / "commit-msg").write_text("#!/bin/sh\nexit 0\n")
    for name in (pathlib.Path(GUARD).name, "fixture_env.py"):
        shutil.copy(REPO / "scripts" / "ci" / name, root / "scripts" / "ci" / name)
    env = sealed_env(identity=True, cwd=root)
    git("init", "-q", cwd=root, env=env)
    git("add", "-A", cwd=root, env=env)
    git("commit", "-q", "--no-verify", "-m", "fixture", cwd=root, env=env)
    git("config", "core.hooksPath", ".githooks", cwd=root, env=env)
    if worktree_config:
        git("config", "extensions.worktreeConfig", "true", cwd=root, env=env)
    second = root.parent / "second"
    git("worktree", "add", "-q", "-b", "second", str(second), cwd=root, env=env)
    return root, second, env


def git(*args: str, cwd: pathlib.Path, env: dict[str, str]) -> None:
    subprocess.run([GIT, *args], cwd=cwd, env=env, check=True,
                   capture_output=True, text=True)


def run(root: pathlib.Path, env: dict[str, str]) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(root / GUARD)],
                          cwd=root, env=env, capture_output=True, text=True)


print("every worktree runs the tracked hooks — what the guard says, on real worktrees")

# 1. The state this clone is meant to be in: both worktrees inherit the shared
#    value, which names the tracked directory.
root, second, env = build()
result = run(root, env)
expect("a clone whose worktrees inherit .githooks passes",
       result.returncode == 0, f"exit={result.returncode}: {result.stderr[:200]}")

# 2. THE DEFECT, exactly as it was measured: a per-worktree override pointing at
#    `.git/hooks`, which holds nothing but git's samples. The shared config is
#    untouched and still says `.githooks`, so a guard that reads the shared file
#    -- or asks only the checkout it runs in -- calls this clone healthy.
git("config", "--worktree", "core.hooksPath", str(root / ".git" / "hooks"),
    cwd=second, env=env)
result = run(root, env)
expect("a worktree pointed at .git/hooks fails", result.returncode == 1,
       f"exit={result.returncode}")
expect("and it names the worktree and the value",
       str(second) in result.stderr and ".git/hooks" in result.stderr,
       result.stderr[:200])
expect("and it says how to put it right",
       "--worktree core.hooksPath .githooks" in result.stderr,
       result.stderr[-200:])

# 3. A worktree with the value UNSET while the others have it. Same silence,
#    different cause: git falls back to `.git/hooks` and runs the samples.
git("config", "--worktree", "--unset", "core.hooksPath", cwd=second, env=env)
git("config", "--worktree", "core.hooksPath", "", cwd=second, env=env)
git("config", "--worktree", "--unset", "core.hooksPath", cwd=second, env=env)
git("config", "--unset", "core.hooksPath", cwd=root, env=env)
git("config", "--worktree", "core.hooksPath", ".githooks", cwd=root, env=env)
result = run(root, env)
expect("a worktree without the value fails while another has it",
       result.returncode == 1, f"exit={result.returncode}: {result.stdout[:160]}")
expect("and it says the others have it",
       "unset while" in result.stderr, result.stderr[:200])

# 4. A FRESH CHECKOUT, which is what CI has on every run: nothing set anywhere.
#    This must be silent. A guard that fails here is a guard that is red on
#    every pull request for a reason that has nothing to do with the change.
git("config", "--worktree", "--unset", "core.hooksPath", cwd=root, env=env)
result = run(root, env)
expect("a checkout where nobody installed the hooks is not judged here",
       result.returncode == 0, f"exit={result.returncode}: {result.stderr[:200]}")
expect("and it says which guard owns that question",
       "the_commit_msg_hook_is_wired.py" in result.stdout, result.stdout[:200])

# 5. And it is called by something. A guard nobody runs is the failure mode
#    this repository has paid for three times.
expect("the local gate runs the guard",
       bool(gate_aanroepen(pathlib.Path(GUARD).name)),
       "no line of scripts/ci/local_ci_gate.sh runs it")

print(f"\n{ran - len(fails)} passed, {len(fails)} failed")
sys.exit(1 if fails else 0)
