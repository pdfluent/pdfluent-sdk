#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The rule that splits the structured-extraction set into tuning and holdout (#566).

Two halves of one question, and both are here rather than in the script that
happens to need them first:

  a rule     which page of the benchmark set is tuning and which is held out,
             computed from a seed, so that the split is a consequence of a
             published sentence rather than a choice somebody made on the day

  a seal     one digest over what the manifest NAMES, so that a table published
             months from now can be checked against a digest that was public
             before the numbers existed

WHY TWENTY PERCENT, AND WHY DRAWN BEFORE THE FIRST RUN
======================================================
#574's rule, and the reason is the one `benchmarks/HOLDOUTS.md` opens with: a
holdout is a set nobody has looked at, which is a claim about the past that no
amount of care in the present can establish. Drawn after the first round, it is
a set somebody has already seen the tail of.

Twenty percent of 4,999 pages is a thousand, which is enough that a percentage
point means something and small enough that the tuning half is still the set the
rounds happen on.

WHY THE SPLIT IS BY DOCUMENT AND NOT BY PAGE
============================================
The page is the unit of measurement, and it is the wrong unit to hold out on. A
benchmark set built from multi-page documents has pages that share a template, a
font programme and a table style; page 3 and page 4 of one annual report are not
independent draws. Split by page and a round tuned on page 3 has, in effect,
been tuned on the holdout -- and nothing would report it, because both digests
would be honest.

So the seeded walk is over DOCUMENTS, every page of a document lands on the same
side, and the twenty percent is counted in pages while it is drawn in documents.
That makes the holdout's size approximate -- it lands where the document that
crosses the line puts it -- and an approximate size is the price of a split that
means what it says. The run file records both numbers.

WHY THE SEED IS NOT ANOTHER SET'S SEED
======================================
`holdout_1000_rule.SEED` and `tuning_and_test_rule`'s seed order other
populations. Reusing one here would tie this split's order to theirs, so that a
document's position in this set could be predicted from its position in another
one -- which is not a property anybody wants and not one anybody would notice
going wrong.
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = REPO / "corpus" / "structured" / "DOCLAYNET_TEST.json"

# One manifest and one seed per set. A SECOND SET IS A SECOND DRAW, never an
# extension of the first: a set that grows after its seal was published is a
# set whose seal says nothing (benchmarks/HOLDOUTS.md). And the seeds differ
# for the reason this file's header gives about the other populations -- one
# seed across two sets ties a document's place in each to its place in the
# other, which is a property nobody wants and nobody would notice going wrong.
MANIFESTS = {
    "doclaynet": MANIFEST,
    "pubtables-1m": REPO / "corpus" / "structured" / "PUBTABLES1M.json",
    "parsebench": REPO / "corpus" / "structured" / "PARSEBENCH.json",
}
# A set whose seed exists and whose first draw has not landed yet. The guard
# reads only MANIFESTS, so a row here does not put it in the announced
# not-a-pass state; the round reads `draw_path_for`, so it can already seal
# the set on the rig and bring the manifest home in the evidence. The row
# moves to MANIFESTS in the commit that lands the draw, with the manifest
# itself and the ledger row -- a FIRST_DRAW path that exists in the repo is
# an unchecked manifest. (T6, #574)
FIRST_DRAW = {
    "fintabnet": REPO / "corpus" / "structured" / "FINTABNET.json",
}
SEEDS = {
    "doclaynet": "pdfluent-structured-holdout-2026-09-17",
    "pubtables-1m": "pdfluent-structured-tables-holdout-2026-09-20",
    "parsebench": "pdfluent-structured-parsebench-holdout-2026-09-23",
    # The lane's seeded walk needs this from the day it lands; the MANIFESTS
    # row beside it lands with the first draw, in the commit that also commits
    # the manifest and the ledger row. A registered set with no manifest is a
    # guard that announces it cannot check, so the row is not added before
    # there is a draw to check.
    "fintabnet": "pdfluent-structured-fintabnet-holdout-2026-09-30",
}

SEED = SEEDS["doclaynet"]


def manifest_for(set_name: str) -> pathlib.Path:
    if set_name not in MANIFESTS:
        raise SystemExit(
            f"[structured-holdout] FATAL: `{set_name}` has no manifest and no "
            "seed here. A set drawn under a seed this file does not name is a "
            "split nobody else can redraw.")
    return MANIFESTS[set_name]


def draw_path_for(set_name: str) -> pathlib.Path:
    """Where the draw of this set is written: sealed, or being sealed.

    A set in MANIFESTS is sealed and its manifest is read from there; a set
    in FIRST_DRAW is being drawn for the first time and its manifest is
    written there, on the rig, and read back from the evidence. Anything
    else is the existing FATAL: a set drawn under a seed this file does not
    name is a split nobody else can redraw.
    """
    if set_name in MANIFESTS:
        return MANIFESTS[set_name]
    if set_name in FIRST_DRAW:
        return FIRST_DRAW[set_name]
    return manifest_for(set_name)


def seed_for(set_name: str) -> str:
    if set_name not in SEEDS:
        raise SystemExit(
            f"[structured-holdout] FATAL: `{set_name}` has no seed here.")
    return SEEDS[set_name]

# The share of the set that is sealed. #574's rule, and a number that moves only
# with the ticket that moves it: a holdout that is re-cut is a holdout nobody
# has to explain the shrinking of.
HOLDOUT_SHARE = 0.20


def rank(source: str, document: str, seed: str = SEED) -> str:
    """The position of a document in the seeded walk. Lower sorts first."""
    return hashlib.sha256(f"{seed}|rank|{source}|{document}".encode()).hexdigest()


def documents_of(entries: list[dict]) -> dict[str, list[dict]]:
    """The pages, grouped by the document they came out of, in manifest order."""
    grouped: dict[str, list[dict]] = {}
    for entry in entries:
        grouped.setdefault(entry["document"], []).append(entry)
    return grouped


def split(entries: list[dict], share: float = HOLDOUT_SHARE,
          seed: str = SEED) -> tuple[list[dict], list[dict]]:
    """The set as (tuning, holdout), by the rule above.

    The walk takes documents into the holdout until the next one would carry it
    past the share, and then stops -- rather than taking the one that crosses and
    overshooting. Under-filling is the safe direction: the tuning half is what
    the rounds need to be representative, and a holdout that is a little small
    is still a holdout.
    """
    grouped = documents_of(entries)
    target = share * len(entries)
    holdout: list[dict] = []
    held: set[str] = set()
    for document in sorted(grouped,
                           key=lambda d: rank(grouped[d][0]["source"], d, seed)):
        pages = grouped[document]
        if len(holdout) + len(pages) > target:
            continue
        holdout.extend(pages)
        held.add(document)
    # Manifest order on both sides, not walk order: a manifest is read by people
    # as well as by this file, and the seal is over what it names in the order
    # it names it.
    tuning = [e for e in entries if e["document"] not in held]
    holdout = [e for e in entries if e["document"] in held]
    return tuning, holdout


def digest_of(entries: list[dict]) -> str:
    """One digest over the pages a manifest names, and nothing else.

    Over `source`, `name` and `sha256` per page, in manifest order, and not over
    the file: the question a seal answers is "are these the same pages", and a
    prose edit to the header must not change the answer. The same shape as
    `holdout_1000_rule.digest_of` and `tuning_and_test_rule.digest_of`, so that
    three seals in one ledger are comparable.
    """
    h = hashlib.sha256()
    for entry in entries:
        h.update(f"{entry['source']}\0{entry['name']}\0{entry['sha256']}\n".encode())
    return h.hexdigest()


def load(path: pathlib.Path = MANIFEST) -> dict:
    if not path.is_file():
        raise SystemExit(
            f"[structured-holdout] FATAL: {path.relative_to(REPO)} is not there. "
            "It is written by the run that draws the set: "
            "`python3 scripts/ci/loop/structured_bench.py --set doclaynet --draw`.")
    return json.loads(path.read_text())


def main() -> int:
    """Print the split of the committed manifest, and the seal of each half."""
    manifest = load()
    entries = manifest["pages"]
    tuning, holdout = split(entries)
    print(f"set            {manifest['set']} ({manifest['version']})")
    print(f"pages          {len(entries)} in {len(documents_of(entries))} document(s)")
    print(f"tuning         {len(tuning)} page(s) in {len(documents_of(tuning))} document(s)")
    print(f"holdout        {len(holdout)} page(s) in {len(documents_of(holdout))} "
          f"document(s) = {100 * len(holdout) / len(entries):.1f}%")
    print(f"seed           {SEED}")
    print(f"seal (holdout) {digest_of(holdout)}")
    print(f"seal (tuning)  {digest_of(tuning)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
