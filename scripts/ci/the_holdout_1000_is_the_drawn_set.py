#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The thousand-document holdout is the set the rule draws, and it is clean.

WHY THIS EXISTS

`benchmarks/pdfa/govdocs_holdout_1000.txt` is the set #365 turns into a
published number, and until now it was a list of a thousand names with nothing
behind them: no digests, so nobody could show that the files measured were the
files drawn; and no exclusion, so fifty-two of them are documents the ten
thousand of #377 already holds -- thirty-two in the tuning half, which debugging
may open on any morning.

A published holdout figure is worth exactly the promise that nobody looked at
the set first. That promise cannot be made by a JSON file, so it is checked
instead: the draw (`scripts/ci/holdout_1000_rule.py`) is deterministic and reads
only committed files, and this redraws it on every push.

WHAT IT CHECKS, AND WHAT EACH ONE IS FOR

  1. `corpus/pdfa/PDFA_HOLDOUT_1000.json` is what the rule draws. Every check
     below is implied by this one and is made anyway, because a guard whose
     every failure arrives as "the file differs" is a guard nobody can act on.

  2. Its `digest` is the digest of the documents it names. The digest is what
     benchmarks/HOLDOUTS.md seals and what a later reader compares against, so a
     header that disagreed with its own entries would make the ledger say
     something the file does not.

  3. Every entry carries a name, a SHA-256 and archive coordinates that resolve
     to a source with an `archive_base` (#215, and run 34292223960: a set whose
     provenance block cannot say where the archive IS is a set that materialises
     as a smaller denominator on a machine bought for the hour).

  4. No document of the set is in any tuning set, in either test set, in the
     PDF/A 300 or in either half of the ten thousand -- by NAME and by CONTENT.
     Content, for the reason the ledger already records: GovDocs1 holds the same
     bytes under more than one name, and five documents of the 300 sit in the
     sealed holdout half under their own names. A name-only exclusion would keep
     a document three rounds were tuned on inside the set that says how much of
     that tuning generalised.

  5. The digest table covers the whole list, and nothing beyond it. The list is
     the population; a digest table that grew past it is a set drawn somewhere
     other than in this repository.

  6. The set is registered in benchmarks/HOLDOUTS.md with its draw name, its
     seed, the day it was drawn and the digest of check 2. A held-out set nobody
     registered is a set whose contents can be decided after the results are in.

# FLOOR: a set of fewer than 500 documents is refused outright. Every check
# above passes trivially on an empty manifest, and an empty manifest is the same
# green a correct one gives.

Exit codes:
  0  the committed holdout is the set the rule draws, and nothing has seen it
  1  it is not
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import build_holdout_1000_manifest as builder  # noqa: E402
import holdout_1000_rule as rule  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
LEDGER = REPO / "benchmarks" / "HOLDOUTS.md"

# A holdout smaller than this is a corpus accident rather than a set, and the
# exclusion of #365 takes fifty-two documents out of a thousand. Anything near
# the floor means something other than the exclusion happened.
MINIMUM = 500


def check_redraw(complaints: list[str]) -> dict | None:
    """Check 1: the committed manifest is what the rule produces."""
    try:
        drawn = builder.build()
    except SystemExit as error:
        complaints.append(
            f"the rule could not draw the set at all: {error}. That is exit 1 "
            "and not a pass -- a set nobody could redraw is a set nobody can "
            "check.")
        return None
    body = builder.render(drawn)
    if not builder.MANIFEST.is_file():
        complaints.append(
            f"{builder.MANIFEST.relative_to(REPO)} is missing; run "
            "`python3 scripts/ci/build_holdout_1000_manifest.py write`.")
        return drawn
    if builder.MANIFEST.read_text(encoding="utf-8") != body:
        complaints.append(
            f"{builder.MANIFEST.relative_to(REPO)} is not what the rule draws. "
            "Either a document was edited in by hand, or the rule changed and "
            "the manifest was not redrawn -- and a rule change is a new draw, "
            "which burns this holdout (benchmarks/HOLDOUTS.md).")
    return drawn


def load(complaints: list[str]) -> dict | None:
    if not builder.MANIFEST.is_file():
        return None
    try:
        return json.loads(builder.MANIFEST.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        complaints.append(f"{builder.MANIFEST.name} is not JSON: {error}")
        return None


def check_entries(document: dict, complaints: list[str]) -> None:
    """Checks 2 and 3, on the COMMITTED file rather than on the redraw.

    On the committed file on purpose: a check that reads the rule's own output
    can only ever agree with it, and would report a clean set while the tree
    held a leak.
    """
    entries = document.get("entries") or []
    if len(entries) < MINIMUM:
        complaints.append(
            f"the manifest holds {len(entries)} document(s), under the floor of "
            f"{MINIMUM}. Every other check passes on an empty set, which is the "
            "same green a correct one gives.")
    if document.get("total") != len(entries):
        complaints.append(
            f"`total` says {document.get('total')} and the manifest holds "
            f"{len(entries)} entries.")

    digest = rule.digest_of(entries)
    if document.get("digest") != digest:
        complaints.append(
            f"`digest` is {document.get('digest')} and the documents named "
            f"hash to {digest}. The ledger publishes the header's digest, so a "
            "header that disagrees with its own entries makes the seal a "
            "decoration.")

    sources = document.get("sources") or {}
    for entry in entries:
        name = entry.get("name") or "<unnamed>"
        for field in ("source", "name", "sha256", "zip", "offset", "method",
                      "csize", "bytes"):
            if entry.get(field) is None:
                complaints.append(f"{name}: no {field}")
        source = sources.get(entry.get("source"))
        if source is None:
            complaints.append(
                f"{name}: source {entry.get('source')!r} has no provenance "
                "block, so the document cannot name its licence (#215)")
        elif not source.get("archive_base"):
            complaints.append(
                f"{name}: source {entry.get('source')!r} has no `archive_base`, "
                "so the entry's coordinates resolve to no URL and the set "
                "cannot be materialised")


def check_population(document: dict, complaints: list[str]) -> None:
    """Check 5: the digest table covers the list, and nothing else."""
    try:
        listed = {rule.key_of(name) for name in builder.population()}
        rows = set(builder.read_entries())
    except SystemExit as error:
        complaints.append(str(error))
        return
    for key in sorted(listed - rows)[:5]:
        complaints.append(
            f"{rule.name_of(key)} is in the list and has no digest in "
            f"{builder.ENTRIES.name}")
    for key in sorted(rows - listed)[:5]:
        complaints.append(
            f"{key} has a digest in {builder.ENTRIES.name} and is not in the "
            "list; the list is the population")
    named = {entry["name"] for entry in document.get("entries") or []}
    excluded = {row["name"] for row in document.get("excluded") or []}
    both = named & excluded
    if both:
        complaints.append(
            f"{len(both)} document(s) are both in the set and in its exclusion "
            f"list ({', '.join(sorted(both)[:3])}).")
    if named | excluded != {rule.name_of(key) for key in listed}:
        complaints.append(
            "the set and its exclusion list do not add up to the population. A "
            "document that is in neither is a document that left the draw "
            "without being booked.")


def check_disjoint(document: dict, complaints: list[str]) -> None:
    """Check 4: nothing in the set is in anything that has been looked at."""
    try:
        names, content, claimed = builder.spoken_for()
    except SystemExit as error:
        complaints.append(str(error))
        return
    for entry in document.get("entries") or []:
        if entry.get("name") in names:
            complaints.append(
                f"{entry['name']} is in the holdout and in "
                f"{claimed.get(entry['name'], 'a set already drawn')}. A "
                "document that has been tuned on cannot say how much the "
                "tuning generalised.")
        elif entry.get("sha256") in content:
            complaints.append(
                f"{entry['name']} is in the holdout and its BYTES are in "
                f"{claimed.get(entry['sha256'], 'a set already drawn')} under "
                "another name. This is the leak a name-only exclusion misses.")
    declared = document.get("excluded_from") or []
    if sorted(declared) != sorted(builder.EXCLUDED_FROM):
        complaints.append(
            "`excluded_from` does not name the sets the exclusion actually "
            f"applies ({', '.join(sorted(builder.EXCLUDED_FROM))}). A manifest "
            "that under-declares what it was held out of overstates itself.")


def check_ledger(document: dict, complaints: list[str]) -> None:
    """Check 6: the ledger names the draw, the seed, the date and the seal."""
    if not LEDGER.is_file():
        complaints.append(f"{LEDGER} is missing")
        return
    text = LEDGER.read_text(encoding="utf-8")
    for needed, what in ((rule.DRAW, "the draw's name"),
                         (rule.SEED, "the seed it was drawn under"),
                         (rule.DRAWN, "the day it was drawn"),
                         (document.get("digest") or "", "the seal")):
        if not needed or needed not in text:
            complaints.append(
                f"benchmarks/HOLDOUTS.md does not carry {what} ({needed}). A "
                "held-out set that is not in the ledger is a set whose contents "
                "can be decided after the numbers are in.")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.parse_args()

    complaints: list[str] = []
    check_redraw(complaints)
    document = load(complaints)
    if document is None:
        complaints.append(
            "the manifest could not be read, so nothing below it was checked. "
            "That is exit 1 and not a pass.")
    else:
        check_entries(document, complaints)
        check_population(document, complaints)
        check_disjoint(document, complaints)
        check_ledger(document, complaints)

    if complaints:
        print(f"[holdout1000] FATAL: {len(complaints)} problem(s):",
              file=sys.stderr)
        for complaint in complaints[:30]:
            print(f"  {complaint}", file=sys.stderr)
        if len(complaints) > 30:
            print(f"  … and {len(complaints) - 30} more", file=sys.stderr)
        return 1

    print(f"[holdout1000] OK: {document['total']} of "
          f"{document['population_total']} documents, drawn by the committed "
          f"rule under {rule.SEED}; "
          f"{len(document['excluded'])} excluded by name or by content; "
          "nothing in the set is in a tuning set, a test set, the 300 or "
          "either half of the ten thousand; sealed in benchmarks/HOLDOUTS.md.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
