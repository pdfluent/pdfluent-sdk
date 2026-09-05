#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""An archive tag outlives the branch it replaces, and that is the problem.

A branch without a pull request is deleted by writing a `keep/*` tag over its
tip first, so no commit becomes unreachable. That is the right move for the
work. It is the wrong move for the addresses in it.

A branch is a moving name somebody will eventually delete. A tag is the opposite
-- it is written precisely so nothing can remove it by accident, and
`docs/licensing/history-rewrite-plan.md` treats a tag as a thing to carry over.
So archiving a branch converts "commits that are on a name we may drop" into
"commits we have promised to keep", and if those commits carry the owner's real
address in author or committer, the promise is to keep publishing it.

Measured 05-09-2026 on the twenty branches with no pull request (#314):

    branches with no pull request              20
    whose commits are all a noreply alias       1
    carrying a personal address                19

and on the archive tags that already exist:

    keep/trunk-2026-08-25              67 commits, 67 not an alias
    keep/t2-territories-signoff-back    2 commits,  0

So the sweep this issue asks for cannot be run as written. Nineteen of the
twenty would each become a permanent ref over an address that #261 exists to
stop publishing and #230 exists to remove. This guard is what makes that a
refusal at the moment the tag is written rather than a discovery afterwards.

WHAT IT DOES NOT DO

It does not name the address. This file ships with the tree, and a denylist that
prints what it refuses publishes it in the one place guaranteed to be read --
the rule `commits_use_the_noreply_alias.py` already sets, and this reuses its
`is_allowed` rather than restating it.

It does not judge branches. A branch is allowed to carry whatever history it
carries; the cutover in that guard covers what is committed from now on. This is
only about the promise a tag makes.

# NO-FLOOR: zero archive tags is a real state (they are created one at a time
# and there may be none), so an empty scan is reported as empty rather than
# treated as a scan that found nothing wrong.
"""

from __future__ import annotations

import argparse
import os
import pathlib
import subprocess
import sys

HIER = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HIER))

from commits_use_the_noreply_alias import is_allowed  # noqa: E402

VOORVOEGSEL = "keep/"

# The one tag that already breaks this rule, with the reason it stays.
#
# `keep/trunk-2026-08-25` pins 67 commits of product and measurement work that
# exist on no other reachable ref -- rich-text sub/sup, password-protected XFA,
# the docx column fix, Office export reaching the C ABI and wasm. Deleting the
# tag to satisfy this guard would make all 67 unreachable, which is the failure
# the tag was created to prevent (#314).
#
# So it is recorded, not forgiven, and recorded in ONE direction only: this set
# may shrink and may not grow. It empties when those commits reach master
# through the slices #314 asks for, or when the history rewrite in #230 gives
# them an authorship that can be published.
BEKEND = {
    "keep/trunk-2026-08-25":
        "67 commits reachable through no other ref; deleting the tag to satisfy "
        "this guard would lose the work it was written to keep (#314). Remove "
        "this row when those commits reach master, or when #230 rewrites them.",
}


def _schone_omgeving() -> dict[str, str]:
    """The caller's environment without git's own.

    This guard runs from the pre-push hook, where GIT_DIR and GIT_WORK_TREE name
    the real repository -- and git obeys those over the directory it is pointed
    at. Its own test then asks it about a throwaway repository and gets answers
    about this one. On 25-08-2026 that shape set `core.bare = true` on the real
    repository and stopped everything.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def _git(*args: str) -> str:
    r = subprocess.run(["git", *args], capture_output=True, text=True,
                       env=_schone_omgeving())
    return r.stdout if r.returncode == 0 else ""


def archieftags() -> list[str]:
    return sorted(t for t in _git("tag", "--list", VOORVOEGSEL + "*").split() if t)


def basis() -> str | None:
    """A master to measure against, or nothing.

    Without one there is no range, and a range that does not resolve prints no
    commits and exits 0 -- which reads exactly like a tag that carries none.
    """
    for naam in ("github/master", "origin/master", "master"):
        if _git("rev-parse", "--verify", "--quiet", naam).strip():
            return naam
    return None


def vuile_commits(ref: str, basisref: str) -> list[tuple[str, str]]:
    """(sha, subject) for commits this ref pins that are not on the base.

    Author AND committer: a rebase writes the second, and a commit whose author
    is an alias while its committer is not publishes the address just as widely.
    """
    uit = []
    regels = _git("log", "--format=%H%x09%ae%x09%ce%x09%s", f"{basisref}..{ref}")
    for regel in regels.splitlines():
        deel = regel.split("\t")
        if len(deel) < 4:
            continue
        sha, auteur, committer, onderwerp = deel[0], deel[1], deel[2], deel[3]
        if not (is_allowed(auteur) and is_allowed(committer)):
            uit.append((sha, onderwerp))
    return uit


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--candidate", action="append", default=[],
        help="a ref about to be archived as a keep/ tag; checked like an "
             "existing tag, so the answer arrives before the tag exists",
    )
    # BEKEND is a fact about THIS repository: it names a tag that exists here and
    # refuses to outlive it. Run the guard anywhere else -- and its own test has
    # to, because the refusal cannot be demonstrated on a tree that must stay
    # clean -- and every row is missing by construction, so the expiry check
    # fires on every case and hides what was being tested.
    #
    # An explicit flag rather than sniffing for "is this the real repository":
    # a guard that guesses which tree it is on will one day guess wrong on the
    # tree that matters.
    p.add_argument(
        "--no-register", action="store_true",
        help="ignore the recorded rows; for running against a repository the "
             "register is not about",
    )
    a = p.parse_args()
    register = {} if a.no_register else BEKEND

    basisref = basis()
    if basisref is None:
        print("[archive-tags] FATAL: no master to measure against, so what a tag "
              "adds over it cannot be established. An unresolvable range prints "
              "no commits and exits 0, which is not the same as clean.",
              file=sys.stderr)
        return 1

    tags = archieftags()
    kandidaten = list(a.candidate)

    verlopen = sorted(set(register) - set(tags))
    if verlopen:
        print("[archive-tags] FATAL: a row names a tag that no longer exists:",
              file=sys.stderr)
        for t in verlopen:
            print(f"  {t} -- remove the row; a baseline that outlives its subject "
                  "excuses nothing and hides the next one.", file=sys.stderr)
        return 1

    fout = []
    for ref in tags + kandidaten:
        vuil = vuile_commits(ref, basisref)
        if not vuil:
            continue
        if ref in register:
            print(f"[archive-tags] recorded: {ref} pins {len(vuil)} commit(s) that "
                  f"do not carry the alias -- {register[ref]}")
            continue
        fout.append((ref, vuil))

    if fout:
        print("[archive-tags] FATAL: an archive tag would promise to keep "
              "publishing an address that is not a noreply alias.\n",
              file=sys.stderr)
        for ref, vuil in fout:
            print(f"  {ref}: {len(vuil)} commit(s)", file=sys.stderr)
            for sha, onderwerp in vuil[:5]:
                print(f"    {sha[:9]}  {onderwerp[:60]}", file=sys.stderr)
            if len(vuil) > 5:
                print(f"    ... and {len(vuil) - 5} more", file=sys.stderr)
        print(
            "\nA branch is a name somebody will delete. A tag is written so that "
            "nothing\ndeletes it, and the history-rewrite plan carries tags "
            "across -- so archiving\nthis way turns a droppable address into a "
            "kept one.\n\n"
            "Leave the branch as a branch until its commits can be published, or "
            "land the\nwork on master, where the alias applies. (#314, #261)",
            file=sys.stderr,
        )
        return 1

    gemeten = len(tags) + len(kandidaten)
    if gemeten == 0:
        print("[archive-tags] no archive tag exists and none was offered; nothing "
              "was measured.")
        return 0
    print(f"[archive-tags] OK: {gemeten} ref(s) measured against {basisref}; "
          f"{len(register)} recorded, the rest pin only the alias.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
