#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Keep expensive infrastructure regressions out of ordinary source-only pushes.

Production guards and crate tests still run. This deliberately uses a small
allowlist: infrastructure, dependencies, fixtures, unknown paths or an unreadable
base retain every regression. Existing compiler caches remain responsible for
build reuse; no green verdict is reused across revisions.
"""
import os
import pathlib
import subprocess

# These exercise isolated orchestration/publication fixtures, not SDK behaviour.
INFRA_REGRESSIONS = frozenset({
    "seedtest", "gatepool", "gatelock", "remotegate", "origincheck", "branchmr",
    "gatesnap", "interntst", "gateprep", "buildheal", "regfetch",
})


def deferred_gates(changed):
    if changed is None:
        return set()
    ordinary = all(
        (p.startswith("crates/") and p.endswith(".rs"))
        or (p.startswith("docs/") and p.endswith(".md"))
        or p in {"README.md", "CHANGELOG.md"}
        for p in changed
    )
    return set(INFRA_REGRESSIONS) if ordinary else set()


def a_clean_environment():
    """The caller's environment with every GIT_* variable removed.

    This script runs from the pre-push hook, and a hook exports GIT_DIR,
    GIT_WORK_TREE and GIT_INDEX_FILE. A `git` call that inherits them ignores
    its own `cwd=` and answers about the repository the hook belongs to, so the
    scope of a push from a worktree was computed against another tree entirely
    -- and on 25-08-2026 the same inheritance put `core.bare = true` on the real
    repository and stopped all thirty worktrees.

    Same helper, same reason, as `scripts/ci/mr_staleness.py`.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def changed_paths(root):
    def git(*args):
        return subprocess.check_output(
            ["git", *args], cwd=root, stderr=subprocess.DEVNULL,
            env=a_clean_environment(),
        ).decode()
    try:
        base = os.environ.get("LANDING_BASE")
        if not base:
            base = git("merge-base", "HEAD", "refs/remotes/github/master").strip()
        # Compare the full working tree, not only committed changes. --no-renames
        # keeps both sides of renames in the scope, including deleted inputs.
        changed = git("diff", "--no-renames", "--name-only", "-z", base).split("\0")
        changed += git("ls-files", "--others", "--exclude-standard", "-z").split("\0")
        return {p for p in changed if p}
    except (subprocess.CalledProcessError, OSError, UnicodeError):
        return None


if __name__ == "__main__":
    root = pathlib.Path(__file__).resolve().parents[2]
    print(" ".join(sorted(deferred_gates(changed_paths(root)))))
