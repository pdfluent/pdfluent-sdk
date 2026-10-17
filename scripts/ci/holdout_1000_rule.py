#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which documents the thousand-document holdout holds, in one importable place.

WHY THIS EXISTS

`benchmarks/pdfa/govdocs_holdout_1000.txt` is the oldest holdout in the
repository and the weakest: a list of names, drawn by hand before
`corpus/pdfa/PDFA_CORPUS_SELECTION.md` existed, with no digests and no rule.
`benchmarks/HOLDOUTS.md` books it as "unsealed, never opened", and says in the
same breath why that is worth less than it sounds -- a claim about the past that
no care in the present can establish.

#365 is the run that turns it into a published number, and a number is only
worth what the set behind it is. So this module states, once, the three things
the set needs and the list never had:

  a seal          the digest over what the manifest names, so the documents
                  measured on Thursday can be shown afterwards to be the
                  documents drawn today.
  an exclusion    everything the tuning sets, the test sets and the ten thousand
                  of #377 already hold is out -- by name AND by the bytes behind
                  the name. The list overlaps the ten thousand in fifty-two
                  places, thirty-two of them in the tuning half that debugging
                  may open at any time.
  an order        seeded, so the set is a function of committed files and a
                  stranger can redraw it. `the_holdout_1000_is_the_drawn_set.py`
                  does exactly that on every push.

WHY BY CONTENT AND NOT ONLY BY NAME

The same hazard `pdfa_corpus_rule.content_unique` and
`tuning_and_test_rule.fill` were written for: GovDocs1 stores several thousand
documents twice, and the ledger has already caught one document sitting in two
sets under two names. An exclusion on names alone would leave a document that
three rounds were tuned on inside the set that is meant to say how much of that
tuning generalised.

WHY THE SEED IS NOT THE CORPUS SEED

Because this draw is over a different population -- the list, not the GovDocs1
universe -- and reusing `pdfa_corpus_rule.SEED` would tie the order of this set
to a constant that belongs to the ten thousand. Changing that seed already burns
`pdfa-2026-09-07-v1`; it must not also silently reshuffle this one.
"""

from __future__ import annotations

import hashlib
import re

# The seed and the date are part of the published rule. Changing either draws a
# different set and burns this holdout (benchmarks/HOLDOUTS.md); neither is a
# knob.
SEED = "pdfluent-pdfa-holdout1000-2026-09-09"
DRAWN = "2026-09-09"
RULE_VERSION = "v1"

DRAW = f"pdfa-holdout1000-{DRAWN}-{RULE_VERSION}"

# The only source the list holds. Stated rather than derived, so a name that is
# not a GovDocs1 name is a loud failure instead of a document fetched from
# nowhere.
SOURCE = "govdocs1"

_NAME = re.compile(r"^(\d{3})_(\d{6})\.pdf$")
_KEY = re.compile(r"^(\d{3})/(\d{6})\.pdf$")


def key_of(name: str) -> str:
    """`000_000012.pdf` -> `000/000012.pdf`, the key inside the zip."""
    match = _NAME.match(name)
    if not match:
        raise SystemExit(
            f"[holdout1000] FATAL: {name!r} is not a GovDocs1 name of the form "
            "<zip>_<number>.pdf, so the archive it has to be fetched from "
            "cannot be derived")
    return f"{match.group(1)}/{match.group(2)}.pdf"


def name_of(key: str) -> str:
    """`000/000012.pdf` -> `000_000012.pdf`, the name it carries on disk."""
    match = _KEY.match(key)
    if not match:
        raise SystemExit(
            f"[holdout1000] FATAL: {key!r} is not a GovDocs1 key of the form "
            "<zip>/<number>.pdf")
    return f"{match.group(1)}_{match.group(2)}.pdf"


def rank(source: str, key: str) -> str:
    """The position of a document in the seeded walk. Lower sorts first."""
    return hashlib.sha256(f"{SEED}|rank|{source}|{key}".encode()).hexdigest()


def draw(rows: list[dict], spent_names: set[str],
         spent_content: set[str]) -> tuple[list[dict], list[dict]]:
    """The set, and what the exclusion took out of the population.

    Walked in the seeded order rather than the order of the list, for the same
    reason `pdfa_corpus_rule` gives: the list's order is whoever numbered the
    GovDocs1 crawl, and an order nobody chose is an order nobody can argue with.
    Both halves of the answer are returned, because a draw that quietly shrank
    is a draw whose size nobody can check.
    """
    kept: list[dict] = []
    dropped: list[dict] = []
    seen_content: set[str] = set()
    for row in sorted(rows, key=lambda r: rank(r["source"], r["key"])):
        if row["name"] in spent_names:
            dropped.append(dict(row, why="name is in a set already drawn"))
        elif row["sha256"] in spent_content:
            dropped.append(dict(row, why="its bytes are in a set already drawn"))
        elif row["sha256"] in seen_content:
            dropped.append(dict(row, why="the list holds these bytes twice"))
        else:
            seen_content.add(row["sha256"])
            kept.append(row)
    return kept, dropped


def digest_of(entries: list[dict]) -> str:
    """One digest over the documents a manifest names, and nothing else.

    Over `source`, `name` and `sha256` per entry, in manifest order, and not
    over the file: the question a seal answers is "are these the same
    documents", and a prose edit to the header must not change the answer. Same
    shape as `tuning_and_test_rule.digest_of`, so the two seals are comparable.
    """
    h = hashlib.sha256()
    for entry in entries:
        h.update(f"{entry['source']}\0{entry['name']}\0{entry['sha256']}\n"
                 .encode())
    return h.hexdigest()
