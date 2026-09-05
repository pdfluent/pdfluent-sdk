#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""No tracked file still calls this repository by a name it has retired.

WHY A GUARD AND NOT A SWEEP THAT RAN ONCE

On 05-09-2026 `jasperdew/xfa-native-rust` was transferred and renamed to
`pdfluent/engine` (#341). GitHub redirects the old path, so nothing broke that
afternoon and the 28 tracked files still naming it kept working -- which is the
whole hazard. A redirect makes a stale name cost nothing today and everything on
the day it is withdrawn, and the tree is where the stale names accumulate: five
workflow headers, a Dockerfile release URL, a Homebrew formula, the reaper's
`GITHUB_REPOSITORY`, and three guards holding the slug as a Python constant they
pass to `gh --repo`.

The rename had already produced one wrong answer before the sweep. Within four
hours it closed every landing on the machine: `origin_is_the_source_in_every_
checkout.py` compares each remote against the topology table in CLAUDE.md, the
remotes had moved and the table had not, and every push was refused by a guard
that was right about the mismatch and could say nothing about which half was
stale. A rename is a change to the map, and this file is what makes the tree part
of the map that gets checked.

WHAT IT DOES NOT COVER, AND WHO DOES

Not remotes. `origin_is_the_source_in_every_checkout.py` already asks whether
`origin` here resolves to the source repository of the topology table, which is
the same question asked of a working copy instead of a file. Two guards answering
one question differently is the defect the register guards exist to catch, so
this one stays out of the checkout entirely and reads only tracked bytes.

Not "some repository somewhere is misspelt" either. Only names THIS repository
has actually carried and given up are listed, each with the change that retired
it. A guard that tried to judge every repository URL in the tree would have to
hold an allowlist of everyone else's, which is a list nobody can finish.

WHY THE REPLACEMENT IS NOT WRITTEN DOWN HERE

The row below names the retired path only. What to put instead is read from the
topology table in CLAUDE.md, through the same reader `the_topology_agrees_with_
the_mirror_gate.py` uses -- so the day this repository is renamed again, the table
moves and the advice in this guard's failure output moves with it, without an
edit here that somebody has to remember. If the table stops naming a source at
all, this refuses rather than guessing: a guard that cannot say what the right
answer is has no business failing anyone for the wrong one.

WHY THE OLD NAME APPEARS IN THIS FILE AND ITS TEST

A denylist has to name what it refuses. Unlike the address denylist next door,
a repository path is not something we are keeping out of a crawler's reach -- it
is public, redirected, and printed by GitHub on request. So it is held in clear,
and the two files that must contain it to do their work are exempt by path.

Exit codes:
  0  no tracked file names a retired path
  1  one does, the exemption list is stale, or the scan was too small to have
     looked
"""

from __future__ import annotations

import os
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from the_topology_agrees_with_the_mirror_gate import rows  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]

# Paths this repository has carried and given up, lowercased. One row per
# rename, with the change that retired it, because a reader who hits this has to
# be able to tell a rename from a typo.
RETIRED: dict[str, str] = {
    "jasperdew/xfa-native-rust":
        "transferred to the organisation and renamed on 05-09-2026 (#341)",
}

# The two files that have to contain a retired path to do their work: this
# guard, which must name what it refuses, and its test, which must plant one.
# Nothing else is excused -- a file that needs to talk about the old name can
# say "the name it carried before #341" and mean it just as precisely.
ALLOWED = {
    "scripts/ci/no_tracked_file_names_the_old_repository.py",
    "scripts/ci/test_no_tracked_file_names_the_old_repository.py",
}

# FLOOR: files read >= 500 -- the repository tracks several thousand. `git
# ls-files` run outside a checkout prints nothing and exits 0, and a scan over
# no files reports a clean tree in exactly the words a real one uses. That is
# the failure this has to survive, because it is the silent one.
MINIMUM_FILES = 500

# Enough of a file to tell text from binary. A PDF fixture carries no repository
# names worth reading and decoding one is noise, so the NUL test happens on this
# much and the rest is only read when the test says text.
HEAD_BYTES = 8192


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


def current_name() -> str | None:
    """The source repository of the topology table, as `owner/name`.

    None when the table does not name one, which is a refusal upstream rather
    than a default here.
    """
    for repository, _remote, role in rows():
        if "**source**" not in role:
            continue
        path = repository.strip("`").strip()
        _, _, slug = path.partition("github.com/")
        if slug.count("/") == 1:
            return slug
    return None


def tracked_files(root: str = ".") -> list[str]:
    r = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, env=_git_env()
    )
    if r.returncode != 0:
        return []
    return [p.decode("utf-8", "replace") for p in r.stdout.split(b"\0") if p]


def scan(root: str = ".", retired: dict[str, str] | None = None,
         allowed: set[str] | None = None,
         floor: int = MINIMUM_FILES) -> tuple[int, list[tuple[str, int, str, str]]]:
    """(files read, hits). A hit is (path, line number, retired path, reason).

    `retired` and `allowed` are parameters so the test can prove the mechanism
    against a planted name of its own, on a fixture repository, instead of
    against the tree it is running in.
    """
    terms = RETIRED if retired is None else retired
    excused = ALLOWED if allowed is None else allowed
    read = 0
    hits: list[tuple[str, int, str, str]] = []

    for path in tracked_files(root):
        if path in excused:
            continue
        full = os.path.join(root, path)
        try:
            with open(full, "rb") as f:
                head = f.read(HEAD_BYTES)
                if b"\0" in head:
                    continue
                raw = head + f.read()
        except OSError:
            # A symlink to nowhere, or a path removed between ls-files and here.
            continue
        read += 1
        text = raw.decode("utf-8", "replace")
        # GitHub treats owner and repository names case-insensitively, and a
        # `git clone` of any casing resolves. So does the redirect, so a
        # differently-cased copy of the old path is the same stale name.
        lowered = text.lower()
        for term, reason in terms.items():
            start = 0
            while (at := lowered.find(term, start)) != -1:
                hits.append((path, text.count("\n", 0, at) + 1, term, reason))
                start = at + len(term)

    if read < floor:
        raise SystemExit(
            f"[old-repo-name] FATAL: {read} file(s) read, below the floor of "
            f"{floor}.\nA scan over nothing reports a clean tree in the same "
            "words as a real one.\nUsually the working directory is not the "
            "repository root, or the checkout is empty."
        )
    return read, hits


def main() -> int:
    if not RETIRED:
        print(
            "[old-repo-name] FATAL: no retired path is listed, so this guard "
            "accepts everything.\nAn empty denylist is not a clean tree.",
            file=sys.stderr,
        )
        return 1

    now = current_name()
    if now is None:
        print(
            "[old-repo-name] SKIPPED (not a pass): the topology table in "
            "CLAUDE.md names no\nsource repository, so this guard cannot say "
            "what the right name is. Nothing was\nchecked -- fix the table "
            "first; the_topology_agrees_with_the_mirror_gate.py\nis the guard "
            "that holds it.",
            file=sys.stderr,
        )
        return 1

    if now.lower() in RETIRED:
        print(
            f"[old-repo-name] FATAL: the topology table names `{now}` as the "
            "source, and this\nguard lists that same path as retired. One of "
            "the two is out of date and this\ncannot tell which.",
            file=sys.stderr,
        )
        return 1

    for path in sorted(ALLOWED):
        if not (REPO / path).is_file():
            print(
                f"[old-repo-name] FATAL: {path} is excused from the scan and "
                "does not exist.\nAn exemption outliving its file is how a real "
                "hit gets skipped later under the\nsame name.",
                file=sys.stderr,
            )
            return 1

    read, hits = scan()

    if hits:
        print(
            f"[old-repo-name] {len(hits)} tracked line(s) still name a path "
            "this repository has retired:\n",
            file=sys.stderr,
        )
        for path, line, term, reason in hits[:40]:
            print(f"  {path}:{line}  {term}  -- {reason}", file=sys.stderr)
        if len(hits) > 40:
            print(f"  ... and {len(hits) - 40} more", file=sys.stderr)
        print(
            f"\nThe repository is `{now}` (the source row of the topology table "
            "in CLAUDE.md).\nGitHub redirects the old path, so these lines work "
            "today and stop working on the\nday the redirect is withdrawn or "
            "the old name is claimed by somebody else --\nwhich is why they are "
            "refused now rather than found then.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[old-repo-name] OK: {read} tracked text file(s) read, "
        f"{len(RETIRED)} retired path(s) checked, none present; the source is "
        f"`{now}`."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
