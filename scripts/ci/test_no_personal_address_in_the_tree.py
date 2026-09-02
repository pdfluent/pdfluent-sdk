#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""Does the tree guard find a planted term, and leave the innocent alone?

The term it plants is its own, not the real one. A test that had to write the
forbidden address into a file to prove the guard finds it would put that address
in the tree -- which is the thing being prevented. So `scan()` takes the
forbidden set as a parameter and this test passes a synthetic one.

Two things that a plain "does it find it" test would miss, and both have bitten
this family of guards before:

  - the floor. A scan over no files prints a clean result. Here that is not a
    hypothetical: `git ls-files` outside a repository exits 0 with no output.
  - the real forbidden set being empty. A denylist with nothing in it accepts
    everything and says so in the words of a pass.
"""

from __future__ import annotations
import sys as _sys, pathlib as _pathlib
_sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parent))
from fixture_env import sealed_env, inside_the_sandbox

import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HIER = Path(__file__).resolve().parent
sys.path.insert(0, str(HIER))
from no_personal_address_in_the_tree import (  # noqa: E402
    FORBIDDEN, _masker, scan,
)

# The planted term. Never the real one -- see the module docstring.
GEPLANT = "victim@example.org"
GEPLANT_HASH = {hashlib.sha256(GEPLANT.encode()).hexdigest(): "the planted term"}

# Addresses that must survive untouched. The tree carries 183 of these and a
# guard that judged them would be unmaintainable; these stand for that.
ONSCHULDIG = [
    "dtolnay@gmail.com",
    "10383561+jasperdew@users.noreply.github.com",
    "noreply@pdfluent.com",
    # Same domain as the planted term, different mailbox. A guard matching on
    # the domain rather than the whole string would take this one too.
    "someone-else@example.org",
    # Same mailbox, different domain, for the mirror of that mistake.
    "victim@example.com",
]


def _git_env(cwd=None) -> dict[str, str]:
    # Sealed rather than merely GIT_*-stripped: dropping GIT_* stops a
    # fixture READING the real repository, not WRITING to the real config.
    # A fixture's `git config user.email t@t` reached a real worktree that
    # way and stamped a test identity onto every later rebase there. (#297)
    return sealed_env(cwd=cwd)


def _git(wd: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(wd), "-c", "core.hooksPath=/nonexistent", *args],
        capture_output=True, text=True, env=_git_env(cwd=wd),
    )


def main() -> int:
    fouten: list[str] = []

    # A guard whose denylist is empty passes everything, and says "none present"
    # while doing it. That state has to be impossible, not merely unlikely.
    if not FORBIDDEN:
        fouten.append("the real forbidden set is empty, so the guard accepts everything")

    # The mask has to be unreadable as an address and still recognisable as one.
    gemaskeerd = _masker("someone@example.org")
    if "someone" in gemaskeerd or "example" in gemaskeerd:
        fouten.append(f"the mask leaks the token it is masking: {gemaskeerd}")
    if not gemaskeerd.endswith(".org"):
        fouten.append(f"the mask drops the part that makes a hit recognisable: {gemaskeerd}")

    with tempfile.TemporaryDirectory() as tmp:
        wd = Path(tmp)
        if _git(wd, "init", "--initial-branch=master", ".").returncode != 0:
            fouten.append("could not create the fixture repository")
            print("[test-tree-address] FAIL", file=sys.stderr)
            return 1

        # A clean tree, with every innocent address in it.
        (wd / "docs").mkdir()
        (wd / "docs" / "authors.md").write_text(
            "# Authors\n" + "".join(f"- {a}\n" for a in ONSCHULDIG), encoding="utf-8"
        )
        # A binary file, which must be skipped rather than decoded into noise.
        (wd / "blob.bin").write_bytes(b"\x00\x01\x02" + GEPLANT.encode() + b"\x00")
        _git(wd, "add", "-A")

        gelezen, treffers = scan(str(wd), forbidden=GEPLANT_HASH, floor=1)
        if treffers:
            fouten.append(
                f"found {len(treffers)} hit(s) in a tree that carries only innocent "
                f"addresses: {[t[0] for t in treffers]}"
            )
        if gelezen < 1:
            fouten.append("read no files from a tree that has some")

        # Now plant it, in a tracked file.
        (wd / "docs" / "report.md").write_text(
            f"All commits authored by `{GEPLANT}`.\n", encoding="utf-8"
        )
        _git(wd, "add", "-A")
        _, treffers = scan(str(wd), forbidden=GEPLANT_HASH, floor=1)
        if not treffers:
            fouten.append("did not find the planted term in a tracked file")
        else:
            pad, nr, masker, _ = treffers[0]
            if pad != "docs/report.md" or nr != 1:
                fouten.append(f"named the wrong place for the planted term: {pad}:{nr}")
            if GEPLANT in masker or "victim" in masker:
                fouten.append(f"printed the term it found instead of a mask: {masker}")

        # UNTRACKED IS NOT SCANNED, AND THAT IS THE POINT. The guard reads
        # `git ls-files`, so a term in a file nobody committed is not a
        # publication. Checking the working directory instead would fail on
        # scratch files and get switched off.
        (wd / "scratch.md").write_text(f"{GEPLANT}\n", encoding="utf-8")
        _, treffers_na = scan(str(wd), forbidden=GEPLANT_HASH, floor=1)
        if len(treffers_na) != len(treffers):
            fouten.append("counted an untracked file, which is not a publication")

        # DEEP IN A LARGE FILE, WHICH IS WHERE THE READ CAP USED TO HIDE IT.
        # The guard read the first megabyte of each file and stopped. Two
        # megabytes of padding and then the term passed with a green line, on a
        # tree that carries sixteen files over that size. Reading a fixed prefix
        # is indistinguishable from reading the file, right up to the moment it
        # is not.
        (wd / "docs" / "big.md").write_text(
            ("padding\n" * 300_000) + f"contact: {GEPLANT}\n", encoding="utf-8"
        )
        _git(wd, "add", "-A")
        _, treffers_diep = scan(str(wd), forbidden=GEPLANT_HASH, floor=1)
        if not any(t[0] == "docs/big.md" for t in treffers_diep):
            fouten.append(
                "missed the term two megabytes into a tracked file, which is a "
                "read cap dressed as a clean tree"
            )
        (wd / "docs" / "big.md").unlink()
        _git(wd, "rm", "--cached", "-q", "docs/big.md")

        # A binary file stays skipped now that the whole file is read: the NUL
        # test happens on the first block, not on a truncated prefix.
        (wd / "big.bin").write_bytes(b"\x00" + b"A" * (2 << 20) + GEPLANT.encode())
        _git(wd, "add", "-A")
        _, treffers_bin = scan(str(wd), forbidden=GEPLANT_HASH, floor=1)
        if any(t[0] == "big.bin" for t in treffers_bin):
            fouten.append("decoded a binary file instead of skipping it")
        (wd / "big.bin").unlink()
        _git(wd, "rm", "--cached", "-q", "big.bin")

        # The floor. A scan that reads almost nothing has not looked, and must
        # not report a clean tree.
        try:
            scan(str(wd), forbidden=GEPLANT_HASH, floor=10_000)
            fouten.append("passed a scan that read fewer files than its floor")
        except SystemExit as e:
            if "floor" not in str(e).lower():
                fouten.append("failed on the floor without naming it")

    if fouten:
        print(f"[test-tree-address] FAIL: {len(fouten)} case(s):", file=sys.stderr)
        for f in fouten:
            print(f"  - {f}", file=sys.stderr)
        return 1

    print(
        f"[test-tree-address] OK: {len(ONSCHULDIG)} innocent address(es) left alone, "
        "a planted term found and masked -- including two megabytes into a file "
        "-- untracked files ignored, binaries skipped, and the floor refuses a "
        "scan that read too little."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
