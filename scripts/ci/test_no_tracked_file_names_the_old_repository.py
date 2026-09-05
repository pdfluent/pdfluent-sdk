#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""Does the retired-name guard find a planted path, and leave lookalikes alone?

The path it plants is a synthetic one, not the real retired path: `scan()` takes
the denylist as a parameter for exactly that reason, so this test proves the
mechanism instead of proving that one particular string is present.

The lookalikes are not decoration. This repository's tree carries the bare word
`xfa-native-rust` in three JSON schema `$id`s, in prose, and in a GitLab token
name, and none of those is a repository path. A guard that matched the name half
of a slug would fail on all of them the day it was switched on, and a guard that
always fails is switched off within the week.

The cases that a plain "does it find it" test would miss, each of which has cost
this family of guards something before:

  - the floor. `git ls-files` outside a repository exits 0 with no output, and a
    scan over nothing reports a clean tree in the words of a real one.
  - a read cap. The address guard read the first megabyte of each file and
    passed a term planted after two.
  - an exemption outliving its file, which silently un-scans whatever is written
    at that path next.
  - the denylist and the topology table drifting into each other, so the guard
    demands the very name it forbids.
"""

from __future__ import annotations

import sys as _sys
import pathlib as _pathlib

_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env  # noqa: E402

import subprocess  # noqa: E402
import sys  # noqa: E402
import tempfile  # noqa: E402
from pathlib import Path  # noqa: E402

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from no_tracked_file_names_the_old_repository import (  # noqa: E402
    ALLOWED, RETIRED, current_name, scan,
)

REPO = HERE.parents[1]

# The planted path. Never the real retired one -- see the module docstring.
PLANTED = "someone/a-repository-that-was-renamed"
PLANTED_SET = {PLANTED: "the planted path"}

# Paths and words that must survive untouched. These stand for what the tree
# actually carries: the repository's old NAME without an owner, another owner's
# repository of the same name, and the same owner's other repositories.
INNOCENT = [
    "https://xfa-native-rust/schemas/benchmark_config.json",
    "the PAT is called a-repository-that-was-renamed-cli",
    "someone-else/a-repository-that-was-renamed",
    "someone/a-different-repository",
]


def _git(wd: Path, *args: str) -> subprocess.CompletedProcess:
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a fixture
    # READING the real repository, not WRITING to the real config (#297).
    return subprocess.run(
        ["git", "-C", str(wd), "-c", "core.hooksPath=/nonexistent", *args],
        capture_output=True, text=True, env=sealed_env(cwd=wd),
    )


def main() -> int:
    failures: list[str] = []

    # A denylist with nothing in it accepts everything, and says "none present"
    # while doing it. That state has to be impossible, not merely unlikely.
    if not RETIRED:
        failures.append("the real retired set is empty, so the guard accepts everything")

    # Every retired path is stored lowercased, because the scan lowercases the
    # text it searches. An upper-case entry would never match anything.
    for term in RETIRED:
        if term != term.lower():
            failures.append(f"the retired path `{term}` is not lowercased, so it can never match")

    # The replacement is read from the topology table rather than written here,
    # so the table has to actually answer -- and it must not answer with a name
    # this guard forbids.
    now = current_name()
    if not now:
        failures.append("the topology table in CLAUDE.md names no source repository")
    elif now.lower() in RETIRED:
        failures.append(f"the table names `{now}` as the source and the guard calls it retired")

    # An exemption that outlives its file un-scans whatever is written at that
    # path next, under a name that still reads like a decision.
    for path in sorted(ALLOWED):
        if not (REPO / path).is_file():
            failures.append(f"{path} is excused from the scan and does not exist")

    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            failures.append("could not create the fixture repository")
            print("[test-old-repo-name] FAIL", file=sys.stderr)
            return 1

        # A clean tree, carrying every lookalike.
        (wd / "docs").mkdir()
        (wd / "docs" / "notes.md").write_text(
            "# Notes\n" + "".join(f"- {s}\n" for s in INNOCENT), encoding="utf-8"
        )
        # A binary, which must be skipped rather than decoded into noise.
        (wd / "blob.bin").write_bytes(b"\x00\x01\x02" + PLANTED.encode() + b"\x00")
        _git(wd, "add", "-A")

        read, hits = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if hits:
            failures.append(
                f"found {len(hits)} hit(s) in a tree that carries only lookalikes: "
                f"{[(h[0], h[1]) for h in hits]}"
            )
        if read < 1:
            failures.append("read no files from a tree that has some")

        # Now plant it, in a tracked file.
        (wd / "docs" / "release.md").write_text(
            f"Download from https://github.com/{PLANTED}/releases/latest\n",
            encoding="utf-8",
        )
        _git(wd, "add", "-A")
        _, hits = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if not hits:
            failures.append("did not find the planted path in a tracked file")
        else:
            path, line, term, _ = hits[0]
            if path != "docs/release.md" or line != 1:
                failures.append(f"named the wrong place for the planted path: {path}:{line}")
            if term != PLANTED:
                failures.append(f"reported `{term}` instead of the path it matched")

        # CASE. GitHub resolves an owner and a repository name case-insensitively,
        # and so does the redirect, so a differently-cased copy of the old path is
        # the same stale name and not a different one.
        (wd / "docs" / "mixed.md").write_text(
            f"See github.com/{PLANTED.title()} for the old tree.\n", encoding="utf-8"
        )
        _git(wd, "add", "-A")
        _, mixed = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if not any(h[0] == "docs/mixed.md" for h in mixed):
            failures.append("missed a differently-cased copy of the planted path")

        # EXCUSED PATHS ARE NOT SCANNED. The guard and its test have to contain
        # the retired path to do their work; nothing else does.
        _, excused = scan(str(wd), retired=PLANTED_SET,
                          allowed={"docs/release.md", "docs/mixed.md"}, floor=1)
        if excused:
            failures.append(
                f"scanned a file it was told to excuse: {[h[0] for h in excused]}"
            )

        # UNTRACKED IS NOT SCANNED, AND THAT IS THE POINT. The guard reads
        # `git ls-files`, so a path in a file nobody committed is not a
        # reference the repository publishes.
        before = len(hits) + len([h for h in mixed if h[0] == "docs/mixed.md"])
        (wd / "scratch.md").write_text(f"{PLANTED}\n", encoding="utf-8")
        _, after = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if len(after) != before:
            failures.append("counted an untracked file, which the repository does not publish")

        # DEEP IN A LARGE FILE, which is where a read cap hides. Reading a fixed
        # prefix is indistinguishable from reading the file, right up to the
        # moment it is not.
        (wd / "docs" / "big.md").write_text(
            ("padding\n" * 300_000) + f"clone https://github.com/{PLANTED}.git\n",
            encoding="utf-8",
        )
        _git(wd, "add", "-A")
        _, deep = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if not any(h[0] == "docs/big.md" for h in deep):
            failures.append(
                "missed the path two megabytes into a tracked file, which is a "
                "read cap dressed as a clean tree"
            )
        (wd / "docs" / "big.md").unlink()
        _git(wd, "rm", "--cached", "-q", "docs/big.md")

        # A binary stays skipped now that the whole file is read: the NUL test
        # happens on the first block, not on a truncated prefix.
        (wd / "big.bin").write_bytes(b"\x00" + b"A" * (2 << 20) + PLANTED.encode())
        _git(wd, "add", "-A")
        _, binary = scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=1)
        if any(h[0] == "big.bin" for h in binary):
            failures.append("decoded a binary file instead of skipping it")
        (wd / "big.bin").unlink()
        _git(wd, "rm", "--cached", "-q", "big.bin")

        # The floor. A scan that reads almost nothing has not looked, and must
        # not report a clean tree.
        try:
            scan(str(wd), retired=PLANTED_SET, allowed=set(), floor=10_000)
            failures.append("passed a scan that read fewer files than its floor")
        except SystemExit as e:
            if "floor" not in str(e).lower():
                failures.append("failed on the floor without naming it")

    if failures:
        print(f"[test-old-repo-name] FAIL: {len(failures)} case(s):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"[test-old-repo-name] OK: {len(INNOCENT)} lookalike(s) left alone, a "
        "planted path found in any casing -- including two megabytes into a file "
        "-- excused paths skipped, untracked files ignored, binaries skipped, "
        f"the floor refuses a scan that read too little, and the replacement "
        f"`{now}` comes from the topology table."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
