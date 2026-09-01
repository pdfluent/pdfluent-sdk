#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.

"""No tracked file publishes an address we decided not to publish.

`commits_use_the_noreply_alias.py` covers the author and committer fields. It
does not cover the tree, and the tree is the easier place to leak: a commit's
identity is set once in a config, while a file is written by hand, by an agent
writing a report, or by a guard that lists the very thing it forbids.

Found on 31-08-2026, after the identity half was already done: eight tracked
files on master carry the owner's personal address as ordinary text, in
benchmark and cleanup reports that assert "all commits authored by <address>".
On the branch behind #1543 there are ten, one of which is `contributors.toml` --
a file whose whole purpose is to record who wrote what, holding the address in
plaintext in a tree that is going public. Fixing the commit fields while the same
string sits in the tree fixes nothing.

WHY HASHES, AND WHAT THAT DOES AND DOES NOT BUY

A denylist has to name what it refuses, and this file ships with the tree. Naming
the address here would publish it in the one file guaranteed to be read.

So the terms are held as SHA-256 of the lowercased string. The guard extracts
every address-shaped token from every tracked text file, hashes it, and refuses a
match. The forbidden string never appears, and neither does the offending token
in the output -- failures name the file, the line and a masked form.

This is not secrecy and should not be sold as it. An address is low-entropy: a
person who already suspects which one it is can confirm it with one hash. What it
buys is that the address is not *harvestable* -- not sitting in a public file for
a crawler to lift, which is the actual exposure being closed. That is the whole
claim.

The mechanism generalises. Any term that must not appear in a public tree --
a partner name, a hostname, an internal path -- can be added as a digest, which
is the shape the older `geen_interne_zaken.py` denylist should have had: that one
holds three partner names, a private address range and a build machine's hostname
in plaintext, in a file whose whole subject is that those must not be published.

WHAT THIS GUARD DOES NOT COVER TODAY, SAID OUT LOUD

Only address-shaped tokens are extracted, so only addresses can be caught, and
only when they are written as an address. `jasper [at] example [dot] nl` is not
matched by the tokeniser and is not hashed to anything, so it passes -- measured,
not assumed, on 31-08-2026. A hash denylist cannot fix that without hashing every
spelling of every term, which is a list nobody can finish. What it does cover is
the leak that actually happens: an address written out, by hand or by a tool,
because writing it was the natural thing to do.

The
internal terms are a real and separate problem and this guard is green in spite
of them, not because of them. Measured on master, 31-08-2026: `/mnt/storagebox`
in 93 tracked files, the runner tag in 80, the build machine's hostname in 2, a
private address in 1, a partner name in 5. Adding those digests here today would
turn the job red on 170-odd files nobody is authorised to bulk-edit in this
change, and a guard that always fails is switched off within the week. They go in
as the cleanup lands, term by term, and until then a green tick here means "no
forbidden address", never "nothing internal in the tree".

Exit codes:
    0  no tracked file carries a forbidden term
    1  one does, or the scan was too small to have looked
"""

from __future__ import annotations

import hashlib
import os
import re
import subprocess
import sys

# SHA-256 of the lowercased terms that must not appear in a tracked file.
#
# One line per term, with the reason. The digest is the whole point: adding a
# term here does not publish it.
FORBIDDEN: dict[str, str] = {
    # The owner's personal address. #261: harvesting addresses out of public
    # repositories is routine and automated, and this one was decided against
    # for commit fields on 30-08-2026. The tree is the same publication.
    "0de40f6171552d141ef375a4926feeaac5f8574e6255f2b455b8b13402c842e2":
        "the owner's personal address (#261)",
}

# Address-shaped tokens. Deliberately the only thing tokenised: the tree carries
# 183 distinct addresses, almost all of them upstream crate authors in lockfiles
# and licence texts, and a scan that tried to judge those would be an allowlist
# nobody could maintain. A hash comparison does not have to judge anything.
ADRES = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")

# FLOOR: files read >= 500 -- the repository tracks several thousand. `git
# ls-files` in the wrong directory prints nothing and exits 0, and a scan over
# no files reports a clean tree in exactly the words a real one uses. That is
# the failure this guard has to survive, because it is the silent one.
MINIMUM_BESTANDEN = 500

# Enough of a file to tell text from binary. Binaries carry no addresses worth
# reading and decoding one is noise, so the NUL test happens on this much and the
# rest of the file is only read when the test says text.
#
# THERE IS NO READ CAP BEYOND THIS. There was one, of a megabyte, on the
# reasoning that an address past the first megabyte is not the kind of leak this
# looks for. That reasoning was wrong and the check proved it: on 31-08-2026 an
# address planted after a megabyte of padding in a tracked file passed the guard
# with a clean green line. Sixteen tracked files here are over a megabyte and the
# largest is five, so the whole tree is 89 MB -- reading all of it costs seconds
# and removes a gap that anyone who read this file could have used.
KOP_BYTES = 8192


def _git_env() -> dict[str, str]:
    """git without the caller's repository-location variables.

    Same reason as the identity guard: inside a hook GIT_DIR and GIT_WORK_TREE
    are absolute and inherited, and every subprocess then works on the caller's
    repository instead of this one.
    """
    weg = {
        "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR", "GIT_NAMESPACE",
        "GIT_CEILING_DIRECTORIES", "GIT_PREFIX",
    }
    return {k: v for k, v in os.environ.items() if k not in weg}


def _masker(token: str) -> str:
    """Enough of an address to recognise, not enough to harvest.

    A failure has to tell the reader what to remove. Echoing the token would put
    the address in a CI log and, once someone pastes the log, back into the tree.
    """
    lokaal, _, domein = token.partition("@")
    stukken = domein.split(".")
    tld = stukken[-1] if stukken else ""
    return f"{lokaal[:1]}***@***.{tld}"


def tracked_files(root: str = ".") -> list[str]:
    r = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, env=_git_env()
    )
    if r.returncode != 0:
        return []
    return [p.decode("utf-8", "replace") for p in r.stdout.split(b"\0") if p]


def scan(root: str = ".", forbidden: dict[str, str] | None = None,
         floor: int = MINIMUM_BESTANDEN) -> tuple[int, list[tuple[str, int, str, str]]]:
    """(files read, hits). A hit is (path, line number, masked token, reason).

    `forbidden` is a parameter so the test can prove the mechanism against a
    planted term of its own. A test that had to plant the real one would have to
    contain it, which is the thing being prevented.
    """
    termen = FORBIDDEN if forbidden is None else forbidden
    gelezen = 0
    treffers: list[tuple[str, int, str, str]] = []

    for pad in tracked_files(root):
        vol = os.path.join(root, pad)
        try:
            with open(vol, "rb") as f:
                kop = f.read(KOP_BYTES)
                # Binary: no addresses worth reading, and decoding one is noise.
                # Tested before the rest is read, so a 3 MB PDF costs 8 kB.
                if b"\0" in kop:
                    continue
                ruw = kop + f.read()
        except OSError:
            # A symlink to nowhere, or a path removed between ls-files and here.
            continue
        gelezen += 1
        tekst = ruw.decode("utf-8", "replace")
        if "@" not in tekst:
            continue
        # One pass over the whole text, not one per line. The line number is
        # only needed for a hit, and hits are rare by construction -- counting
        # newlines up front costs more than every hit this guard will ever find.
        for m in ADRES.finditer(tekst):
            digest = hashlib.sha256(m.group(0).lower().encode()).hexdigest()
            reden = termen.get(digest)
            if reden:
                nr = tekst.count("\n", 0, m.start()) + 1
                treffers.append((pad, nr, _masker(m.group(0)), reden))

    if gelezen < floor:
        raise SystemExit(
            f"[tree-address] FATAL: {gelezen} file(s) read, below the floor of "
            f"{floor}.\nA scan over nothing reports a clean tree in the same "
            "words as a real one.\nUsually the working directory is not the "
            "repository root, or the checkout is empty."
        )
    return gelezen, treffers


def main() -> int:
    if not FORBIDDEN:
        print(
            "[tree-address] FATAL: the forbidden set is empty, so this guard "
            "accepts everything.\nAn empty denylist is not a clean tree.",
            file=sys.stderr,
        )
        return 1

    gelezen, treffers = scan()

    if treffers:
        print(
            f"[tree-address] {len(treffers)} tracked line(s) carry a term this "
            "repository decided not to publish:\n",
            file=sys.stderr,
        )
        for pad, nr, masker, reden in treffers[:40]:
            print(f"  {pad}:{nr}  {masker}  -- {reden}", file=sys.stderr)
        if len(treffers) > 40:
            print(f"  ... and {len(treffers) - 40} more", file=sys.stderr)
        print(
            "\nThe token is masked on purpose: printing it would put it in a CI "
            "log.\nOpen the line, remove the address, and say what it was "
            "instead -- the\nreports that carry it are asserting who authored "
            "something, which reads\njust as well without the address.\n\n"
            "The history behind those files is a separate question, written up "
            "for the\nowner under #261.",
            file=sys.stderr,
        )
        return 1

    print(
        f"[tree-address] OK: {gelezen} tracked text file(s) read, "
        f"{len(FORBIDDEN)} forbidden term(s) checked, none present."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
