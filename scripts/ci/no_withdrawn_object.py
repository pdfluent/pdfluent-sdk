#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Refuse a withdrawn object by CONTENT, over every commit a push adds.

#260. A document was once taken out of the public history and the history was
rewritten. `every_document_is_registered.py` catches a new file in the working
tree, but not `git checkout <old-sha> -- <path>`: the file then arrives under a
name that somebody who does not know its history would simply add to SOURCES.md.
A name-based guard cannot see that.

So this one looks at the object, not the name. Renaming does not help, moving
does not help, and it does not matter which commit of the push carries it --
`git rev-list --objects` enumerates everything the range introduces, not only
what stands at the tip.

WHY THE SHAS ARE NOT IN THIS REPOSITORY
=======================================
The list lives outside the tree, like the customer and partner names in
`geen_interne_zaken.py`. #260 says it in so many words: the exact SHAs and the
reachable routes must not be repeated in any public repository, commit message
or generated document. A guard that ships its own blocklist tells you precisely
where the withdrawn thing can still be fetched.

For the same reason a hit names the path and not the SHA. A CI log is not a
private place.

If the list is missing this is SKIPPED (not a pass), not green. There is
deliberately no built-in fallback -- that would write the SHA down here after
all.
"""
from __future__ import annotations
import os
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
LIST_PATH = os.environ.get(
    "PDFLUENT_WITHDRAWN_OBJECTS",
    os.path.expanduser("~/.config/pdfluent/withdrawn-objects.txt"),
)


def sealed_env() -> dict[str, str]:
    """The caller's environment with every GIT_* variable removed.

    Inside a pre-push hook GIT_DIR and GIT_INDEX_FILE point at the real
    repository, and git then ignores the directory you point it at. On
    25-08-2026 that made a test set `core.bare = true` on the real repository.

    This is the sixth copy of this function in scripts/ci. There should be one;
    that is a cleanup for whoever is not busy writing this file, and not a reason
    to use the unsafe variant here.
    """
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True,
                          text=True, check=False, env=sealed_env())


def blocklist() -> set[str]:
    """The blocklist, or SystemExit when it is not there."""
    try:
        with open(LIST_PATH, encoding="utf-8") as f:
            lines = [r.strip().lower() for r in f]
    except OSError:
        raise SystemExit(
            "no_withdrawn_object: SKIPPED (not a pass) -- the list of withdrawn "
            f"objects is missing at {LIST_PATH}.\nPoint PDFLUENT_WITHDRAWN_OBJECTS "
            "at it, or create the file with one SHA per line (`#` is a comment). "
            "The list does NOT belong in this repository: it would publish the "
            "very thing it exists to stop."
        )
    return {r for r in lines if r and not r.startswith("#")}


def push_range() -> str:
    """What this push adds.

    Same reasoning as `every_commit_since_the_cutoff_is_signed.py`: the upstream
    if there is one, otherwise everything not on master, otherwise the whole
    branch. That logic now sits in two places and should sit in one; that is its
    own cleanup, not a reason to do something different here.
    """
    upstream = git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}")
    if upstream.returncode == 0 and upstream.stdout.strip():
        return f"{upstream.stdout.strip()}..HEAD"
    for ref in ("github/master", "origin/master"):
        base = git("merge-base", "HEAD", ref)
        if base.returncode == 0 and base.stdout.strip():
            return f"{base.stdout.strip()}..HEAD"
    return "HEAD"


def main() -> int:
    blocked = blocklist()
    r = push_range()
    out = git("rev-list", "--objects", r)
    if out.returncode != 0:
        print(f"[withdrawn] SKIPPED (not a pass): `git rev-list --objects {r}` "
              f"failed:\n  {out.stderr.strip()[:200]}", file=sys.stderr)
        return 1

    hits: list[str] = []
    seen = 0
    for line in out.stdout.splitlines():
        if not line:
            continue
        seen += 1
        parts = line.split(maxsplit=1)
        sha = parts[0].lower()
        if sha in blocked:
            # The path yes, the SHA no -- see the module docstring.
            hits.append(parts[1] if len(parts) > 1 else "<no path>")

    if hits:
        print(f"[withdrawn] {len(hits)} object(s) in {r} are on the list of "
              "what was taken out of the public history.\n"
              "They are not recognisable by name; this is the object itself. "
              "Most likely via `git checkout <old-sha> -- <path>`.\n",
              file=sys.stderr)
        for path in sorted(set(hits))[:25]:
            print(f"  {path}", file=sys.stderr)
        print("\nDrop the commit that adds it. See #260 for why a rename does "
              "not solve this.", file=sys.stderr)
        return 1

    # Zero objects over a range that does have commits is impossible. Every
    # commit brings at least itself and its tree. So that is not an empty push but
    # a range measuring something other than it should -- and reporting green over
    # zero objects examined is exactly the shape these gates exist to refuse.
    commits = git("rev-list", "--count", r)
    n = int(commits.stdout.strip()) if commits.returncode == 0 and commits.stdout.strip() else 0
    if n and not seen:
        print(f"[withdrawn] SKIPPED (not a pass): {r} holds {n} commit(s) but "
              "`rev-list --objects` returned nothing. Nothing was examined then, "
              "and that is not green.", file=sys.stderr)
        return 1

    print(f"[withdrawn] OK: {seen} object(s) from {n} commit(s) in {r}, none "
          f"of them on the list of {len(blocked)} withdrawn object(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
