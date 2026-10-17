#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The four sets of #461 are the sets the rule draws, and the test sets are clean.

WHY THIS EXISTS

A tuning set and a test set are worth exactly one thing: the promise that the
second was not used to build the first. Nothing in a manifest can carry that
promise on its own -- a JSON file is a list of names, and a list of names can be
edited by anybody who finds a document inconvenient on the morning of a round.

So the promise is checked instead of made. The rule
(`scripts/ci/tuning_and_test_rule.py`) is deterministic and reads only files
that are already in the tree, so this guard can redraw all four sets on every
push and compare them, byte for byte, with the manifests that are committed. A
document swapped out by hand does not survive that, and neither does a seed
changed quietly: a redraw under a new seed produces four different files, all
four of which show up here as a diff.

WHAT IT CHECKS, AND WHAT EACH ONE IS FOR

  1. The four manifests are what the rule draws. Everything else below is a
     property this one already implies -- and is checked anyway, because a guard
     whose failures all arrive as one opaque "the file differs" is a guard whose
     output nobody can act on.

  2. Each manifest's `digest` is the digest of the documents it names. The
     digest is what benchmarks/HOLDOUTS.md publishes and what a later round is
     compared against, so a header that disagrees with its own entries would
     make the ledger say something the file does not.

  3. Every entry has a name, a SHA-256 and a licence reachable through its
     source (#215), and every entry that carries archive coordinates resolves
     to a source that says where the archive IS. The second half was added
     after run 34292223960: two manifests describe `govdocs1`, only the PDF/A
     one carries `archive_base`, and a plain dictionary merge let the other win
     -- so the fetcher raised KeyError on all 180 GovDocs1 documents of the test
     set and produced 120 of 300. The set was internally consistent and every
     digest was right; what was missing was a URL, which nothing here was
     looking at. An unfetchable set is not a set, and finding that out costs an
     instance.

  4. The tuning sets contain what the published rounds already stand on: the
     PDF/A 300 (`pdfa_fidelity_inputs_300.json`), and for extraction the whole
     pinned gate set together with the 121-document comparison list of #390.
     Without that the baselines do not carry over and every number restarts.

  5. No test document is in any tuning set, in the ten thousand of #377, or in
     the other test set -- by NAME and by CONTENT. Content, because upstream
     holds the same bytes under more than one name and the PDF/A 300 predates
     the corpus rule: five of the 300 sit in the sealed holdout half under their
     own names, and a name-only check would leave them in a test set.

  6. Both test sets are registered in benchmarks/HOLDOUTS.md with the draw's
     seed, its date and the digest of check 2. A held-out set nobody registered
     is a set whose contents can be decided after the results are in.

  7. Every burn booked in the ledger has been acted on: the document is gone
     from every set, and the row names the document the redraw put in its place.
     A burn that is only written down is the state #478 was opened for -- the
     ledger said a document had been looked at and the set still held it, with
     no path from the one to the other. Check 1 catches the manifest that was
     never redrawn; this one catches the row that was never finished, which
     check 1 cannot see because an empty `replacement` column changes nothing
     about what the rule draws.

Exit codes:
  0  the four sets are the drawn sets and the test sets are disjoint
  1  they are not
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import build_tuning_and_test_manifests as builder  # noqa: E402
import tuning_and_test_rule as rule  # noqa: E402

REPO = HERE.parents[1]
HOLDOUTS = REPO / "benchmarks" / "HOLDOUTS.md"
COMPARISON_LIST = (REPO / "benchmarks" / "extraction" / "comparison"
                   / "gate-tail.txt")
SHA256 = re.compile(r"^[0-9a-f]{64}$")

TUNING = ("pdfa-tuning", "extraction-tuning")
TEST = ("pdfa-test", "extraction-test")


def load(label: str, complaints: list[str]) -> dict | None:
    path = builder.OUTPUTS[label]
    if not path.is_file():
        complaints.append(f"{path.relative_to(REPO)} is missing")
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        complaints.append(f"{path.relative_to(REPO)} is not JSON: {exc}")
        return None


def check_redraw(built: dict, complaints: list[str]) -> None:
    for label, document in built.items():
        path = builder.OUTPUTS[label]
        wanted = builder.render(document)
        found = path.read_text(encoding="utf-8") if path.is_file() else None
        if found == wanted:
            continue
        complaints.append(
            f"{path.relative_to(REPO)} is not what "
            f"scripts/ci/tuning_and_test_rule.py draws under seed "
            f"{rule.SEED!r}. Regenerate it with "
            "`python3 scripts/ci/build_tuning_and_test_manifests.py --write` "
            "and, if the set really has to change, book the redraw in "
            "benchmarks/HOLDOUTS.md -- a redrawn test set is a burnt one.")


def check_entries(label: str, document: dict, complaints: list[str]) -> None:
    entries = document.get("entries")
    if not isinstance(entries, list) or not entries:
        complaints.append(f"{label}: no `entries` -- this is not a corpus manifest")
        return
    sources = document.get("sources", {})
    for entry in entries:
        name = entry.get("name")
        if not name:
            complaints.append(f"{label}: an entry has no name")
            continue
        if not SHA256.match(entry.get("sha256") or ""):
            complaints.append(f"{label}: {name} carries no SHA-256")
        source = sources.get(entry.get("source", ""), {})
        if not (entry.get("licence") or source.get("licence")):
            complaints.append(
                f"{label}: {name} resolves to no licence, in its entry or in "
                "its source (#215)")
        if entry.get("offset") is not None and not source.get("archive_base"):
            complaints.append(
                f"{label}: {name} carries archive coordinates but its source "
                f"{entry.get('source')!r} does not say where the archive is. "
                "fetch_pdfa_corpus.py cannot materialise it, and the run that "
                "finds that out has already bought an instance.")
    want = rule.TEST_SIZE if document.get("role") == "test" else rule.TUNING_SIZE
    if len(entries) != want:
        complaints.append(
            f"{label}: {len(entries)} documents, not {want}. A short set is a "
            "smaller denominator on every axis it is reported on.")
    found = rule.digest_of(entries)
    if document.get("digest") != found:
        complaints.append(
            f"{label}: the header's digest is {document.get('digest')} and the "
            f"entries hash to {found}. benchmarks/HOLDOUTS.md publishes the "
            "header's digest, so the two disagreeing means the ledger names a "
            "set that is not in the file.")


def check_subsets(on_disk: dict, complaints: list[str]) -> None:
    """What the published rounds stand on has to be inside the tuning sets."""
    pdfa_300 = {row["sha256"] for row in builder.pdfa_300()}
    tuning = {e["sha256"] for e in on_disk["pdfa-tuning"]["entries"]}
    missing = pdfa_300 - tuning
    if missing:
        complaints.append(
            f"{len(missing)} of the PDF/A 300 are not in the tuning set. The "
            "300 are what benchmarks/pdfa/baseline.json holds; a tuning set "
            "without them makes every recorded number start again.")

    gate = {row["sha256"] for row in builder.extraction_set("gate")}
    extraction = {e["sha256"] for e in on_disk["extraction-tuning"]["entries"]}
    missing = gate - extraction
    if missing:
        complaints.append(
            f"{len(missing)} document(s) of the pinned gate set are not in the "
            "extraction tuning set; benchmarks/extraction/baseline.json is "
            "recorded on them.")

    if COMPARISON_LIST.is_file():
        by_name = {e["name"] for e in on_disk["extraction-tuning"]["entries"]}
        wanted = [line.strip() for line in
                  COMPARISON_LIST.read_text(encoding="utf-8").splitlines()
                  if line.strip() and not line.startswith("#")]
        absent = sorted(set(wanted) - by_name)
        if absent:
            complaints.append(
                f"{len(absent)} of the {len(wanted)} documents of the #390 "
                "comparison are not in the extraction tuning set: "
                + ", ".join(absent[:5])
                + ". The published seven-tool table is measured on them, and a "
                  "tuning set that cannot see them cannot fix what it reports.")
    else:
        complaints.append(
            f"{COMPARISON_LIST.relative_to(REPO)} is missing, so it cannot be "
            "checked that the comparison documents are inside the tuning set.")


def check_disjoint(on_disk: dict, complaints: list[str]) -> None:
    spent_names: set[str] = set()
    spent_content: set[str] = set()
    for label in TUNING:
        spent_names |= {e["name"] for e in on_disk[label]["entries"]}
        spent_content |= {e["sha256"] for e in on_disk[label]["entries"]}
    for row in builder.pdfa_half("tuning"):
        spent_names.add(row["name"])
        spent_content.add(row["sha256"])

    seen_names: set[str] = set()
    seen_content: set[str] = set()
    for label in TEST:
        names = {e["name"] for e in on_disk[label]["entries"]}
        content = {e["sha256"] for e in on_disk[label]["entries"]}
        for kind, overlap in (("name", names & spent_names),
                              ("content", content & spent_content)):
            if overlap:
                complaints.append(
                    f"{label}: {len(overlap)} document(s) are also in a tuning "
                    f"set or in the ten thousand, by {kind}: "
                    + ", ".join(sorted(overlap)[:5])
                    + ". A test set that shares one document with the set the "
                      "fixes were chosen on reports nothing -- not less, "
                      "nothing, because there is no way to say afterwards how "
                      "much of the result came from the overlap.")
        for kind, overlap in (("name", names & seen_names),
                              ("content", content & seen_content)):
            if overlap:
                complaints.append(
                    f"{label}: {len(overlap)} document(s) are already in the "
                    f"other capability's test set, by {kind}. A document burnt "
                    "on one capability would silently leave the other's test "
                    "set too.")
        seen_names |= names
        seen_content |= content


def check_ledger(on_disk: dict, complaints: list[str]) -> None:
    if not HOLDOUTS.is_file():
        complaints.append("benchmarks/HOLDOUTS.md is missing")
        return
    text = HOLDOUTS.read_text(encoding="utf-8")
    for label in TEST:
        digest = on_disk[label]["digest"]
        if digest not in text:
            complaints.append(
                f"{label}: benchmarks/HOLDOUTS.md does not carry the digest "
                f"{digest}. A held-out set that is not in the ledger is a set "
                "whose contents can be decided after the numbers are in.")
    for needed, what in ((rule.SEED, "the draw seed"),
                         (rule.DRAWN, "the date it was drawn")):
        if needed not in text:
            complaints.append(
                f"benchmarks/HOLDOUTS.md does not name {what} ({needed}).")


def check_burns(on_disk: dict, complaints: list[str]) -> None:
    """A burn in the ledger is a document out of the set and a name in its place.

    The rule reads this same table, so a burn that was booked and not acted on
    already fails check 1 -- the redraw no longer holds the document and the
    committed manifest does. What is checked here is the half of the swap the
    redraw cannot see: which set the row says it happened in, and whether the
    row names the document that actually took the burnt one's place. A row that
    stops at "burnt" leaves the ledger unable to say what the set became.
    """
    burns = rule.burns()
    if not burns:
        return
    by_draw = {document.get("draw"): (label, document)
               for label, document in on_disk.items()}
    everywhere = {name: {entry["name"] for entry in document["entries"]}
                  for name, document in on_disk.items()}

    for burn in burns:
        where = f"benchmarks/HOLDOUTS.md line {burn['line']}"
        still_in = sorted(label for label, names in everywhere.items()
                          if burn["document"] in names)
        if still_in:
            complaints.append(
                f"{where}: {burn['document']} is booked burnt and is still in "
                + ", ".join(still_in)
                + ". A burnt document that is still in the set it was burnt out "
                  "of makes the ledger a note rather than a rule; redraw with "
                  "`python3 scripts/ci/build_tuning_and_test_manifests.py "
                  "--write`.")
        if burn["set"] not in by_draw:
            complaints.append(
                f"{where}: {burn['document']} is booked burnt out of "
                f"{burn['set']!r}, which is not a set any of the four manifests "
                "draws. The set names are "
                + ", ".join(sorted(name for name in by_draw if name))
                + ".")
            continue
        label, document = by_draw[burn["set"]]
        if not burn["replacement"]:
            complaints.append(
                f"{where}: {burn['document']} is booked burnt out of {label} "
                "with no replacement named. The redraw has already put one "
                "document in its place -- the ledger has to say which, or it "
                "records that the set changed without recording what it became.")
        elif burn["replacement"] not in everywhere[label]:
            complaints.append(
                f"{where}: {burn['document']} is booked as replaced by "
                f"{burn['replacement']}, which is not in {label}. The "
                "replacement is whatever the redraw drew, not a document "
                "somebody chose; run the builder and read the name out of the "
                "diff.")
        booked = document.get("burnt_and_replaced")
        if not isinstance(booked, list) or not any(
                row.get("document") == burn["document"] for row in booked):
            complaints.append(
                f"{label}: its `burnt_and_replaced` block does not carry "
                f"{burn['document']}, which the ledger books against this set. "
                "The block is written by the draw out of the ledger, so the two "
                "disagreeing means the manifest was not redrawn after the burn.")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.parse_args()

    complaints: list[str] = []

    # Two questions, and they are asked of two different things on purpose. The
    # redraw asks whether the committed files are what the rule produces. Every
    # check after it asks what the COMMITTED files say -- not what the rule would
    # have said -- because a check that reads the rule's own output can only ever
    # agree with it, and would report a clean split while the tree held a leak.
    check_redraw(builder.build(), complaints)

    on_disk: dict[str, dict] = {}
    for label in builder.OUTPUTS:
        document = load(label, complaints)
        if document is None:
            continue
        on_disk[label] = document
        check_entries(label, document, complaints)

    if set(on_disk) == set(builder.OUTPUTS):
        check_subsets(on_disk, complaints)
        check_disjoint(on_disk, complaints)
        check_ledger(on_disk, complaints)
        check_burns(on_disk, complaints)
    else:
        complaints.append(
            "not all four manifests could be read, so the subset and "
            "disjointness checks were not run. That is exit 1 and not a pass: a "
            "split nobody could check is a split nobody has.")

    if complaints:
        print(f"[tuning-sets] FATAL: {len(complaints)} problem(s):",
              file=sys.stderr)
        for complaint in complaints[:30]:
            print(f"  {complaint}", file=sys.stderr)
        if len(complaints) > 30:
            print(f"  … and {len(complaints) - 30} more", file=sys.stderr)
        return 1

    burnt = rule.burns()
    print("[tuning-sets] OK: "
          f"{rule.TUNING_SIZE} tuning + {rule.TEST_SIZE} test documents per "
          "capability, drawn by the committed rule; the 300, the gate set and "
          "the 121 of the comparison are inside the tuning sets; neither test "
          "set shares a name or a byte with any of them; "
          f"{len(burnt)} burn(s) in the ledger, each out of its set and each "
          "with its replacement named.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
