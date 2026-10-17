#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""No tracked file sits under the measurement-evidence roots.

WHY A GUARD AND NOT AN ENTRY IN .gitignore

Train Q carried `meas/p4l-3f/` -- six proof logs of one measurement -- into
the repository, and the `meas/p4l-3l/` evidence of the next job was one landing
away from following it. Both were already archived where evidence belongs,
under `~/.local/share/pdfluent/meas/<job>/`; the tracked copies were hygiene,
not loss. But `meas/` was never in `.gitignore`, so nothing said no:
`meas-out/` has been ignored since #574 and `meas/` simply never joined it.

An ignore entry stops the next accident and does nothing about the tracked
files already there -- and it never says anything at all. A guard refuses, out
loud, naming the paths, which is the half that teaches the next author where
evidence goes instead of letting them discover the archive layout from a
reverted landing.

WHAT IT CHECKS

Every path `git ls-files` reports -- the tracked tree, which is what the
repository publishes -- against the two evidence roots, `meas/` and
`meas-out/`. Any path under either is a failure, and every such path is named.

WHAT IT DOES NOT CHECK

Untracked files. An evidence log sitting on a worktree's disk, ignored, is
exactly where it should be; the repository publishes only what is tracked, so
only that is judged. (The old-name guard next door reads tracked bytes for the
same reason.)

A directory that merely shares the name deeper in the tree. The rule is about
the two roots the archive layout mirrors -- `meas/<job>/` and the `meas-out/`
scratch dir -- not the word. The day a nested `meas/` carries evidence, its
root joins ROOTS with its reason, the way a retired repository path would.

WHY A GIT THAT CANNOT ANSWER IS A FAILURE

`git ls-files` outside a repository exits 0 with no output, and a scan over
nothing reports a clean tree in the words of a real one. That is the failure
this family of guards has been bitten by before, so a git that cannot answer
is refused rather than believed.

Exit codes:
  0  no tracked path sits under meas/ or meas-out/
  1  one does, or git could not answer
"""

from __future__ import annotations

from collections.abc import Callable
import os
import subprocess
import sys

# The evidence roots, as top-level path prefixes. One row per root, with the
# reason it is evidence and not source.
ROOTS: tuple[str, ...] = (
    # Per-job proof logs, archived under ~/.local/share/pdfluent/meas/<job/>.
    "meas/",
    # Scratch measurement output, ignored since #574.
    "meas-out/",
)


def _git_env() -> dict[str, str]:
    """git without the caller's repository-location variables.

    Inside a hook GIT_DIR and GIT_WORK_TREE are absolute and inherited, and
    every subprocess then answers about the caller's repository rather than the
    one being scanned.
    """
    drop = {
        "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES", "GIT_PREFIX",
    }
    return {k: v for k, v in os.environ.items() if k not in drop}


def is_evidence_path(path: str) -> bool:
    """Whether a tracked path sits under one of the evidence roots."""
    return path.startswith(ROOTS)


def tracked_files(root: str = ".") -> list[str] | None:
    """Every tracked path under root, or None when git cannot answer."""
    try:
        r = subprocess.run(
            ["git", "ls-files", "-z"], cwd=root, capture_output=True,
            env=_git_env(),
        )
    except OSError:
        return None
    if r.returncode != 0:
        return None
    return [p.decode("utf-8", "replace") for p in r.stdout.split(b"\0") if p]


def scan(root: str = ".",
         predicate: Callable[[str], bool] | None = None) -> list[str]:
    """Tracked evidence paths under root, sorted.

    `predicate` is a parameter so the test can prove the mechanism -- including
    the mutant with the path check removed -- against a fixture repository
    instead of against the tree it is running in.
    """
    check = is_evidence_path if predicate is None else predicate
    tracked = tracked_files(root)
    if tracked is None:
        raise SystemExit(
            "[tracked-evidence] FATAL: git could not list the tracked files, "
            "so nothing was checked.\nA scan over nothing reports a clean "
            "tree in the same words as a real one.\nUsually the working "
            "directory is not inside a repository."
        )
    return sorted(p for p in tracked if check(p))


def main() -> int:
    hits = scan()

    if hits:
        print(
            f"[tracked-evidence] {len(hits)} tracked file(s) under meas/ or "
            "meas-out/:\n",
            file=sys.stderr,
        )
        for path in hits[:40]:
            print(f"  {path}", file=sys.stderr)
        if len(hits) > 40:
            print(f"  ... and {len(hits) - 40} more", file=sys.stderr)
        print(
            "\nMeasurement evidence is archived under "
            "~/.local/share/pdfluent/meas/<job/>, not tracked.\nUntrack what "
            "is listed (`git rm -r --cached <path>`) once the bytes are "
            "archived;\nboth roots are in .gitignore, so they stay untracked "
            "afterwards.",
            file=sys.stderr,
        )
        return 1

    print("[tracked-evidence] OK: no tracked file sits under meas/ or "
          "meas-out/.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
