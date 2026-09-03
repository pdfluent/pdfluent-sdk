#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""NOTICE has to match the tree under it, and its own second copy.

Two things, both because NOTICE is a register, and a register that no longer
touches its subject is worse than none: it gets believed.

1. THE TWO COPIES AGREE. `bindings/dotnet/src/PDFluent/NOTICE` travels inside the
   NuGet package. What a .NET user reads about our licences has to be what the
   repository says; two versions means one of them is untrue and nobody can see
   which.

2. EVERY PATH IT NAMES EXISTS. A line about `.claude/skills/caveman` left behind
   after that directory is gone -- or written before it arrives -- describes
   something the reader does not have. Same shape as a guard still watching a
   deleted file, or a ledger that never shrinks.

WHAT COUNTS AS A PATH
=====================
Only something whose first segment is a real entry in the root. Without that
rule `skills/caveman/` counts -- the UPSTREAM repository's layout, named to say
where something came from -- and this file would demand we rebuild someone
else's tree.
"""
from __future__ import annotations
import os
import pathlib
import re
import sys

REPO = pathlib.Path(os.environ.get("PDFLUENT_NOTICE_ROOT")
                    or pathlib.Path(__file__).resolve().parents[2])
ROOT_NOTICE = REPO / "NOTICE"
COPY_NOTICE = REPO / "bindings/dotnet/src/PDFluent/NOTICE"

# A path-shaped word: segments joined by `/`, or a bare file with an extension.
PATH_LIKE = re.compile(r"(?<![\w/.-])((?:\.?[\w.-]+/)+[\w.-]+/?|[\w-]+\.[A-Za-z][\w.]*)")


def paths(text: str, roots: set[str]) -> list[str]:
    found = []
    for line in text.splitlines():
        # A URL is not a path in this tree.
        without_url = re.sub(r"https?://\S+", " ", line)
        for m in PATH_LIKE.finditer(without_url):
            cand = m.group(1).rstrip("/.,;:")
            first = cand.split("/")[0]
            if first in roots:
                found.append(cand)
    return found


def main() -> int:
    if not ROOT_NOTICE.exists() or not COPY_NOTICE.exists():
        print("[notice] SKIPPED (not a pass): one of the two NOTICE files is "
              f"missing ({ROOT_NOTICE.exists()=}, {COPY_NOTICE.exists()=}), so nothing was "
              "compared.", file=sys.stderr)
        return 1

    a, b = ROOT_NOTICE.read_bytes(), COPY_NOTICE.read_bytes()
    if a != b:
        print("[notice] the two NOTICE files have diverged.\n"
              "  NOTICE                                (repository)\n"
              "  bindings/dotnet/src/PDFluent/NOTICE   (travels in the NuGet package)\n\n"
              "  A .NET user reads the second. Two versions means one of them is\n"
              "  untrue, and the reader cannot see which.",
              file=sys.stderr)
        return 1

    roots = {p.name for p in REPO.iterdir()}
    candidates = paths(a.decode("utf-8", "replace"), roots)
    missing = sorted({p for p in candidates if not (REPO / p).exists()})
    if missing:
        print(f"[notice] {len(missing)} path(s) named in NOTICE do not exist here:\n",
              file=sys.stderr)
        for p in missing:
            print(f"  {p}", file=sys.stderr)
        print("\n  A register that no longer touches its subject gets believed.\n"
              "  Remove the line, or land it once the path is there.",
              file=sys.stderr)
        return 1

    # A floor, because zero recognised paths is not "everything checks out" but
    # "nothing was looked at". The number is small -- NOTICE mostly names crates,
    # and a crate name is not a path -- which is exactly why it must not be able
    # to reach zero unnoticed: a broken extractor looks identical to a NOTICE that
    # is correct.
    if not candidates:
        print("[notice] SKIPPED (not a pass): no path recognised in NOTICE at "
              "all. Either the file was gutted or the matching is broken; either "
              "way nothing was checked.", file=sys.stderr)
        return 1

    print(f"[notice] OK: both copies identical, {len(set(candidates))} named "
          "path(s) exist.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
