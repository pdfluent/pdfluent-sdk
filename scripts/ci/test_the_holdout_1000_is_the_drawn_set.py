#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Prove the holdout guard is red on each way the holdout can be spoiled (#365).

A guard that has never been red has never been tested, and this one carries the
whole value of the number #365 publishes: that the thousand documents behind it
were not the documents the rounds were tuned on. So every way that promise can
be broken gets one case here, and each case is a mutation of a real tree rather
than of a fixture that resembles one.

The cases, and what each of them is:

  a tuning document in the holdout         the leak the exclusion exists for.
  the same BYTES under another name        the leak a name-only exclusion misses.
                                           GovDocs1 holds several thousand
                                           documents twice, and five of the
                                           PDF/A 300 sit in the sealed holdout
                                           half under their own names.
  a document dropped from the set          a smaller denominator, reported as
                                           the same measurement.
  a document moved out of `excluded` and
    nowhere else                           the set and its exclusion list stop
                                           adding up to the population, so a
                                           document left the draw unbooked.
  a digest header edited to match a hand-
    edited entry list                      the redraw, and not the digest, is
                                           what makes a manifest checkable.
  the ledger's seal edited                 a ledger that agrees with whatever is
                                           in front of it registers nothing.
  the seed changed without a redraw        a new seed is a new draw, and a new
                                           draw is a burn that goes through the
                                           ledger.
  a source that has lost its archive_base  a set whose digests are all correct
                                           and which cannot be fetched. Run
                                           34292223960 produced 120 of 300 on a
                                           manifest that passed every other
                                           check.
  a digest changed in the coordinates
    table                                  the table is the population's
                                           evidence; a digest edited there
                                           silently renames which bytes were
                                           measured.
  a document in the coordinates table
    that the list does not name            the list is the population, and a
                                           table that grew past it is a set
                                           drawn somewhere else.
  `excluded_from` under-declared           a manifest that says it was held out
                                           of less than it was overstates
                                           nothing, but a manifest that says it
                                           was held out of MORE than the code
                                           applies overstates everything -- so
                                           the two must be the same list.

Exit codes:
  0  the guard is green on the tree and red on every mutation
  1  it is not
"""

from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/the_holdout_1000_is_the_drawn_set.py"
BUILDER = "scripts/ci/build_holdout_1000_manifest.py"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
import holdout_1000_rule as rule  # noqa: E402

# Everything the guard and the builder read. Listed rather than copied wholesale
# so that a new input to the rule shows up here as a missing file instead of as
# a case that silently measured the real repository.
NEEDED = (
    GUARD,
    BUILDER,
    "scripts/ci/holdout_1000_rule.py",
    "scripts/ci/build_pdfa_corpus_manifest.py",
    "scripts/ci/pdfa_corpus_rule.py",
    "benchmarks/pdfa/govdocs_holdout_1000.txt",
    "corpus/pdfa/pdfa_holdout1000_entries.tsv",
    "corpus/pdfa/PDFA_HOLDOUT_1000.json",
    "corpus/pdfa/PDFA_TUNING_MANIFEST.json",
    "corpus/pdfa/PDFA_TUNING_1000.json",
    "corpus/pdfa/PDFA_TEST_300.json",
    "crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json",
    "benchmarks/HOLDOUTS.md",
)

MANIFEST = "corpus/pdfa/PDFA_HOLDOUT_1000.json"
TUNING = "corpus/pdfa/PDFA_TUNING_1000.json"
ENTRIES = "corpus/pdfa/pdfa_holdout1000_entries.tsv"
LEDGER = "benchmarks/HOLDOUTS.md"
RULE = "scripts/ci/holdout_1000_rule.py"


def build_sandbox(root: pathlib.Path) -> None:
    for name in NEEDED:
        source = REPO / name
        if not source.is_file():
            raise SystemExit(f"FAIL {name} is missing from the repository")
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    corpus = root / "corpus" / "pdfa"
    for entries in sorted((REPO / "corpus" / "pdfa").glob("pdfa_*_entries.*.tsv")):
        shutil.copy2(entries, corpus / entries.name)


def run(root: pathlib.Path) -> tuple[int, str]:
    done = subprocess.run(
        [sys.executable, str(root / GUARD)],
        capture_output=True, text=True, check=False, cwd=root)
    return done.returncode, done.stdout + done.stderr


def read(root: pathlib.Path, name: str) -> dict:
    return json.loads((root / name).read_text(encoding="utf-8"))


def write(root: pathlib.Path, name: str, document: dict) -> None:
    (root / name).write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main() -> int:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="holdout1000-test-") as tmp:
        root = pathlib.Path(tmp)
        build_sandbox(root)

        code, output = run(root)
        if code != 0:
            print("FAIL the guard is red on an unmutated copy of the tree, so "
                  f"nothing below proves anything:\n{output}")
            return 1

        tuning = read(root, TUNING)
        pristine = {name: (root / name).read_text(encoding="utf-8")
                    for name in (MANIFEST, TUNING, ENTRIES, LEDGER, RULE)}

        def restore() -> None:
            for name, body in pristine.items():
                (root / name).write_text(body, encoding="utf-8")

        def case(what: str, mutate, expect: str) -> None:
            mutate()
            code, output = run(root)
            if code == 0:
                failures.append(f"{what}: the guard stayed green")
            elif expect not in output:
                failures.append(
                    f"{what}: the guard went red, but not about {expect!r}. "
                    f"It said:\n{output}")
            restore()

        def mutate_manifest(change) -> None:
            document = read(root, MANIFEST)
            change(document)
            write(root, MANIFEST, document)

        # THE TWO LEAKS. A name from a tuning set, and then the same document's
        # BYTES under a name no other set uses -- the second is the one an
        # exclusion on names alone lets through.
        case("a tuning document in the holdout",
             lambda: mutate_manifest(lambda d: d["entries"].__setitem__(
                 0, dict(d["entries"][0], name=tuning["entries"][0]["name"],
                         key=tuning["entries"][0]["key"]))),
             "is in the holdout and in")

        case("a tuning document's bytes under another name",
             lambda: mutate_manifest(lambda d: d["entries"].__setitem__(
                 0, dict(d["entries"][0],
                         sha256=tuning["entries"][0]["sha256"]))),
             "BYTES")

        # A SMALLER DENOMINATOR. Dropping a document changes nothing a reader
        # can see in the file; it changes what every percentage is out of.
        case("a document dropped from the set",
             lambda: mutate_manifest(
                 lambda d: d.__setitem__("entries", d["entries"][1:])),
             "not what the rule draws")

        # THE BOOKKEEPING. Everything the population holds is either in the set
        # or in the exclusion list, with a reason.
        case("an excluded document booked nowhere",
             lambda: mutate_manifest(
                 lambda d: d.__setitem__("excluded", d["excluded"][1:])),
             "do not add up to the population")

        # A HAND-EDITED SET WITH A MATCHING HEADER. The digest is recomputed
        # from the entries, so a forger who edits both gets past check 2 -- and
        # not past the redraw, which is why check 1 exists.
        def edit_with_matching_digest(document: dict) -> None:
            document["entries"] = document["entries"][1:]
            document["total"] = len(document["entries"])
            document["digest"] = rule.digest_of(document["entries"])

        case("a hand-edited set whose header was edited to match",
             lambda: mutate_manifest(edit_with_matching_digest),
             "not what the rule draws")

        # THE LEDGER. A seal that follows the file registers nothing.
        case("the ledger's seal edited to something else",
             lambda: (root / LEDGER).write_text(
                 pristine[LEDGER].replace(
                     read(root, MANIFEST)["digest"], "0" * 64),
                 encoding="utf-8"),
             "the seal")

        # THE SEED. A new seed is a new draw, and a new draw is a burn.
        case("the seed changed without a redraw",
             lambda: (root / RULE).write_text(
                 pristine[RULE].replace(rule.SEED, rule.SEED + "-again"),
                 encoding="utf-8"),
             "not what the rule draws")

        # AN UNFETCHABLE SET. Every digest right, no URL to fetch from.
        case("a source with no archive_base",
             lambda: mutate_manifest(
                 lambda d: d["sources"][rule.SOURCE].pop("archive_base", None)),
             "archive_base")

        # THE COORDINATES TABLE, which is the population's evidence.
        def edit_a_digest() -> None:
            lines = pristine[ENTRIES].splitlines()
            cells = lines[1].split("\t")
            cells[1] = "0" * 64
            lines[1] = "\t".join(cells)
            (root / ENTRIES).write_text("\n".join(lines) + "\n",
                                        encoding="utf-8")

        case("a digest changed in the coordinates table",
             edit_a_digest,
             "not what the rule draws")

        def add_a_stranger() -> None:
            cells = pristine[ENTRIES].splitlines()[1].split("\t")
            cells[0] = "999/999999.pdf"
            (root / ENTRIES).write_text(
                pristine[ENTRIES] + "\t".join(cells) + "\n", encoding="utf-8")

        case("a document in the table that the list does not name",
             add_a_stranger,
             "the list does not name")

        # THE DECLARATION. What the manifest says it was held out of has to be
        # what the code holds it out of.
        case("`excluded_from` under-declared",
             lambda: mutate_manifest(
                 lambda d: d.__setitem__("excluded_from",
                                         d["excluded_from"][:-1])),
             "excluded_from")

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        print(f"FAIL {len(failures)} of the mutations were not caught")
        return 1
    print("OK the holdout guard is green on the tree and red on all 11 "
          "mutations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
