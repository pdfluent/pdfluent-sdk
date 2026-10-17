#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The extraction-set comparison finds a drifted name, and nothing else (#448).

A comparison that reports nothing looks exactly like two manifests that agree,
so the cases go in directly: a name the corpus does not carry, a name with a
directory in it, and a set that matches. Then the guard is run over the tree,
because a check that finds everything is as useless as one that finds nothing.

Exit codes:
  0  the comparison reports exactly the names that would fail on the instance
  1  it misses one, or invents one
"""
from __future__ import annotations

import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "the_extraction_set_names_the_corpus_it_is_given.py"

sys.path.insert(0, str(GUARD.parent))
import the_extraction_set_names_the_corpus_it_is_given as guard  # noqa: E402

CORPUS = ["a.pdf", "b.pdf", "c.pdf"]


def main() -> int:
    failures: list[str] = []

    if guard.disagreements(["a.pdf", "b.pdf"], CORPUS):
        failures.append("a set the corpus carries in full must pass")

    missing = guard.disagreements(["a.pdf", "gone.pdf"], CORPUS)
    if len(missing) != 1 or "gone.pdf" not in missing[0]:
        failures.append(
            "a document the corpus does not carry must be named; the extraction "
            f"step refuses the whole set on it. Got {missing}"
        )

    nested = guard.disagreements(["sub/a.pdf"], CORPUS)
    if len(nested) != 1:
        failures.append(
            "the corpus is written flat, so a name with a directory must be "
            f"reported even when its basename exists. Got {nested}"
        )

    if len(guard.disagreements(["x.pdf", "y.pdf"], CORPUS)) != 2:
        failures.append("every drifted name must be reported, not only the first")

    result = subprocess.run(
        [sys.executable, str(GUARD)], capture_output=True, text=True
    )
    if result.returncode != 0:
        failures.append(
            "the guard is red on this tree, so the extraction step would stop on "
            f"the instance:\n{result.stdout}{result.stderr}"
        )

    if failures:
        print("[extraction-set-test] FAIL", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("[extraction-set-test] OK: the comparison bites on drift and only on drift")
    return 0


if __name__ == "__main__":
    sys.exit(main())
