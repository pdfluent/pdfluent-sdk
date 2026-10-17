#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which documents are tuned on and which are held back, in one importable place.

WHY THIS EXISTS

Three PDF/A rounds were tuned on exactly the three hundred documents that then
reported the score, and the extraction comparison picks its fixes out of the
same hundred and twenty-one it publishes. Both numbers are optimistic by
construction, and no amount of care in the rounds can take that back out: the
set that finds the bugs cannot also be the set that says how many are left.

#365 already forbids publishing a number before a holdout run, but it draws the
holdout once and spends it. What was missing is the thing in between -- a set
big enough to debug on, and a second set, drawn at the same time, that says
every round whether the debugging generalised. That is what this rule draws
(#461).

WHAT IT DRAWS

  tuning, 1000 per capability   everything debugging, classification and fixes
                                may look at. It contains what the rounds
                                already stand on, so the baselines carry over:
                                the PDF/A 300 for PDF/A, and the whole pinned
                                gate set -- the 121-document comparison list
                                with it -- for extraction.
  test, 300 per capability      opened only by a measuring rig, reported only
                                as aggregates. Disjoint from every tuning set
                                and from the ten thousand of #377, by name and
                                by content.

WHY A SEEDED ORDER AND NOT A SAMPLE

The same reason `pdfa_corpus_rule.py` gives for the ten thousand: a set drawn by
a rule can be recomputed by a stranger and argued with; a set drawn by a script
that ran once can only be believed. `the_tuning_and_test_sets_are_the_drawn_sets.py`
redraws all four sets from the committed populations on every push and refuses
a manifest that is not what this rule produces, which is what stops a document
from being quietly swapped out because it was inconvenient.

WHY THE QUOTAS

The ten thousand is six parts GovDocs1 to four parts SAFEDOCS, and a thousand
drawn uniformly out of it would be *approximately* that -- approximately being
the word that makes two rounds incomparable when the draw is repeated. The
quota is apportioned exactly, by largest remainder, so the composition of the
thousand is a stated property rather than an outcome.

WHY CONTENT AND NOT NAMES

Upstream holds the same bytes under more than one name, which is why the corpus
rule walks content-unique. The same hazard reappears here one level up: the
PDF/A 300 predates the ten thousand and was drawn by hand, and five of its
documents sit in the sealed holdout half under their own names (recorded in
`benchmarks/HOLDOUTS.md`). Excluding a tuning document from a test draw by name
alone would leave those five in the test set -- tuned on for three rounds, and
counted as evidence that the tuning generalised.

WHY THE BURN LIST IS PART OF THE RULE

A test document that has been looked at is spent, and the ledger has always said
so. What it could not do was act: the draw redrew both test sets from the
committed populations and nothing else, so a burn booked in
`benchmarks/HOLDOUTS.md` left the document exactly where it was, and the only way
to take it out was to edit a manifest by hand -- which is the one edit
`the_tuning_and_test_sets_are_the_drawn_sets.py` exists to refuse. A set was
therefore 299 documents nobody had looked at and one that had been, with no path
from the ledger to the file (#478).

So the burn table in the ledger is an input to the draw. `burns()` reads it, the
builder excludes every burnt document from both test draws -- by name and by the
bytes behind that name, for the same reason the five are excluded by content --
and the seeded walk goes one document further to fill the gap. That makes the
replacement a consequence of the rule rather than a choice: it is the next
document the same walk offers, and anybody with this repository can recompute it.
The ledger names it, and the guard checks that the name it gives is the document
the redraw actually produced.

There is one file and not two on purpose. A burn list beside the ledger would be
a second place to write the same fact, and the failure it invites -- booked in
one, absent from the other -- is precisely the failure that made this ticket.
"""

from __future__ import annotations

import hashlib
import pathlib

# The seed is part of the published rule, and the date is the day the four sets
# were drawn. Changing either draws different sets and burns both test sets;
# neither is a knob. Both are recorded in benchmarks/HOLDOUTS.md.
SEED = "pdfluent-tuning-and-test-2026-09-09"
DRAWN = "2026-09-09"
RULE_VERSION = "v1"

# What each half is worth. A thousand is the smallest set on which a per-document
# defect class is still countable after it has been split by producer and size
# band; three hundred is what the rounds have shown can be measured on the four
# PDF/A axes inside an hour of instance time (#385).
TUNING_SIZE = 1_000
TEST_SIZE = 300

# The composition every draw out of the PDF/A population is apportioned to, from
# `pdfa_corpus_rule.SOURCE_SHARE`. Stated here rather than imported so that a
# change to the ten thousand's own shares does not silently redraw these four
# sets: a redraw is a burn, and it goes through the ledger.
SOURCE_SHARE = {"govdocs1": 6, "safedocs": 4}

# The ledger, and the one table in it this module reads. The columns are matched
# by their headings rather than by position: a table read by position turns a
# column somebody inserted into a silent misreading of every row, and a burn
# misread is a document that stays in a test set.
REPO = pathlib.Path(__file__).resolve().parents[2]
LEDGER = REPO / "benchmarks" / "HOLDOUTS.md"
BURN_COLUMNS = ("document", "set", "date", "replacement", "reason")


def draw_name(capability: str, set_name: str) -> str:
    """The name a drawn set carries in benchmarks/HOLDOUTS.md.

    Derived rather than written down twice: the ledger's rows and the manifests
    have to be talking about the same set, and a name spelled by hand in two
    files is a name that eventually differs in one of them.
    """
    return f"{capability}-{set_name}-{DRAWN}-{RULE_VERSION}"


def _cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def burns(ledger: pathlib.Path | None = None) -> list[dict]:
    """The burnt documents, as the ledger books them.

    A test document that has been looked at is spent; this is where that fact
    enters the draw. The table is found by its heading row, so prose may move
    around it, and a row that does not have the shape of the heading is a loud
    failure: a burn silently dropped on the floor is the failure this whole path
    exists to end.

    Empty is a perfectly good answer -- it means no test document has been
    looked at -- and it is the answer this returned every day before #478.
    """
    path = LEDGER if ledger is None else ledger
    if not path.is_file():
        raise SystemExit(
            f"[tuning-sets] FATAL: {path} is missing, so the draw cannot know "
            "which documents are burnt. An unreadable ledger is not an empty "
            "one.")
    rows: list[dict] = []
    inside = False
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.lstrip().startswith("|"):
            inside = False
            continue
        cells = _cells(line)
        if not inside:
            if [cell.lower() for cell in cells] == list(BURN_COLUMNS):
                inside = True
            continue
        if all(set(cell) <= set("-: ") for cell in cells):
            continue                      # the heading's underline
        if len(cells) != len(BURN_COLUMNS):
            raise SystemExit(
                f"[tuning-sets] FATAL: {path.name} line {number} is in the burn "
                f"table with {len(cells)} column(s) instead of "
                f"{len(BURN_COLUMNS)} ({', '.join(BURN_COLUMNS)}).")
        row = dict(zip(BURN_COLUMNS, (cell.strip("`") for cell in cells)))
        if not row["document"]:
            raise SystemExit(
                f"[tuning-sets] FATAL: {path.name} line {number} books a burn "
                "without naming a document.")
        row["line"] = number
        rows.append(row)
    return rows


def rank(label: str, source: str, key: str) -> str:
    """The position of a document in one seeded walk. Lower sorts first.

    `label` separates the walks: the tuning fill and the two test draws are
    three different orders over overlapping pools, and one order for all three
    would make the test sets the tail of the tuning set -- which is a set that
    moves the moment the tuning set grows.
    """
    return hashlib.sha256(
        f"{SEED}|{label}|{source}|{key}".encode()).hexdigest()


def order(rows: list[dict], label: str) -> list[dict]:
    """`rows` in the seeded order of `label`."""
    return sorted(rows, key=lambda r: rank(label, r["source"], r["key"]))


def quota(want: int, shares: dict[str, int]) -> dict[str, int]:
    """Apportion `want` over `shares` by largest remainder.

    Exactly `want`, and the same answer every time: plain rounding leaves a
    remainder that has to go somewhere, and "wherever the loop happened to be"
    is not a rule anybody can recompute. Ties break on the source name, because
    a tie broken on dictionary order is a tie broken on the Python version.
    """
    total = sum(shares.values())
    if total <= 0:
        raise ValueError("shares must not be empty")
    exact = {name: want * part / total for name, part in shares.items()}
    out = {name: int(value) for name, value in exact.items()}
    left = want - sum(out.values())
    by_remainder = sorted(
        exact, key=lambda name: (-(exact[name] - out[name]), name))
    for name in by_remainder[:left]:
        out[name] += 1
    return out


def fill(pool: list[dict], label: str, per_source: dict[str, int],
         taken_names: set[str], taken_content: set[str]) -> list[dict]:
    """Walk `pool` in seeded order and take what the quota still has room for.

    A document already taken -- under this name or under these bytes -- is
    skipped rather than counted, so a pool that overlaps what is already in the
    set produces a smaller draw and never a duplicate. The caller checks the
    size it got: a draw that came up short is a corpus problem to be reported,
    not a set to be quietly padded from somewhere else.
    """
    room = dict(per_source)
    out: list[dict] = []
    for row in order(pool, label):
        source = row["source"]
        if room.get(source, 0) <= 0:
            continue
        if row["name"] in taken_names or row["sha256"] in taken_content:
            continue
        room[source] -= 1
        taken_names.add(row["name"])
        taken_content.add(row["sha256"])
        out.append(row)
    return out


def digest_of(entries: list[dict]) -> str:
    """One digest over the documents a manifest names, and nothing else.

    Over `name`, `sha256` and `source` per entry, in manifest order, rather than
    over the file: the question this answers is "are these the same documents",
    and a prose edit to the header must not change the answer. The file as a
    whole is sealed separately in benchmarks/HOLDOUTS.md, which answers the other
    question -- "is this the same file nobody has opened".
    """
    h = hashlib.sha256()
    for entry in entries:
        h.update(f"{entry['source']}\0{entry['name']}\0{entry['sha256']}\n"
                 .encode())
    return h.hexdigest()
