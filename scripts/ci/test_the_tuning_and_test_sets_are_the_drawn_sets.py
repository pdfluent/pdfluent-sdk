#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Prove the tuning/test guard is red on each way the split can be broken (#461).

A guard that has never been red has never been tested. This one carries the
whole value of the split -- that the test sets were not used to build the tuning
sets -- so every way that promise can be broken gets one case here, and each
case is a mutation of a real tree rather than a fixture that resembles one.

The cases, and what each of them is:

  a test document swapped for a tuning one   the leak the split exists to stop.
  a test document swapped for a document
    of the OTHER capability's test set       the leak a name-only check would
                                             still catch, kept apart from the
                                             next case on purpose.
  a test document swapped for the same
    bytes under a different name             the leak a name-only check would
                                             NOT catch. Five of the PDF/A 300
                                             sit in the sealed holdout half
                                             under their own names, and that is
                                             how it was found.
  a document dropped from a test set         a smaller denominator, reported as
                                             the same measurement.
  one of the 300 dropped from the tuning set the baselines stop carrying over.
  a digest header edited to match a hand-
    edited entry list                        the redraw, not the digest, is what
                                             makes a manifest checkable.
  the ledger's digest edited                 a register that agrees with whatever
                                             is in front of it registers nothing.
  the seed changed without a redraw          a redraw is a burn and goes through
                                             the ledger.
  a source that has lost its archive_base    a set whose digests are all correct
                                             and which cannot be fetched. Run
                                             34292223960 produced 120 of 300 on
                                             a manifest that passed every other
                                             check here.
  a burn booked and never acted on           the state #478 was opened for: the
                                             ledger says a test document has
                                             been looked at and the set still
                                             holds it.
  a burn whose replacement is not named      the swap recorded without what it
                                             swapped to, so the ledger cannot
                                             say what the set became.
  a burn whose replacement is a document
                                 of choice   the replacement is what the redraw
                                             drew; a name somebody picked is not
                                             it.
  the rule reading no burn list at all       the burnt document comes straight
                                             back into the committed set, which
                                             is what the path costs if it is
                                             ever taken out again.

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
GUARD = "scripts/ci/the_tuning_and_test_sets_are_the_drawn_sets.py"
BUILDER = "scripts/ci/build_tuning_and_test_manifests.py"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
import tuning_and_test_rule as rule  # noqa: E402

# Everything the guard and the builder read. Listed rather than copied wholesale
# so that a new input to the rule shows up here as a missing file rather than as
# a case that silently measured the real repository.
NEEDED = (
    "scripts/ci/the_tuning_and_test_sets_are_the_drawn_sets.py",
    "scripts/ci/build_tuning_and_test_manifests.py",
    "scripts/ci/tuning_and_test_rule.py",
    "corpus/pdfa/PDFA_TUNING_MANIFEST.json",
    "corpus/pdfa/PDFA_TUNING_1000.json",
    "corpus/pdfa/PDFA_TEST_300.json",
    "corpus/extraction/tuning.json",
    "corpus/extraction/EXTRACTION_TUNING_1000.json",
    "corpus/extraction/EXTRACTION_TEST_300.json",
    "crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json",
    "benchmarks/HOLDOUTS.md",
    "benchmarks/extraction/comparison/gate-tail.txt",
)

PDFA_TEST = "corpus/pdfa/PDFA_TEST_300.json"
PDFA_TUNING = "corpus/pdfa/PDFA_TUNING_1000.json"
EXTRACTION_TEST = "corpus/extraction/EXTRACTION_TEST_300.json"
EXTRACTION_TUNING = "corpus/extraction/EXTRACTION_TUNING_1000.json"
LEDGER = "benchmarks/HOLDOUTS.md"
RULE = "scripts/ci/tuning_and_test_rule.py"


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


def swap_in(root: pathlib.Path, into: str, entry: dict) -> None:
    """Replace the first entry of a set with `entry`, keeping the size."""
    document = read(root, into)
    document["entries"] = [entry] + document["entries"][1:]
    write(root, into, document)


def main() -> int:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="tuning-sets-test-") as tmp:
        root = pathlib.Path(tmp)
        build_sandbox(root)

        code, output = run(root)
        if code != 0:
            failures.append(
                "the guard is red on an unmutated copy of the tree, so nothing "
                f"below proves anything:\n{output}")
            for line in failures:
                print(f"FAIL {line}")
            return 1

        pdfa_tuning = read(root, PDFA_TUNING)
        extraction_test = read(root, EXTRACTION_TEST)
        pristine = {name: (root / name).read_text(encoding="utf-8")
                    for name in (PDFA_TEST, PDFA_TUNING, EXTRACTION_TEST,
                                 EXTRACTION_TUNING, LEDGER, RULE)}

        def restore() -> None:
            for name, body in pristine.items():
                (root / name).write_text(body, encoding="utf-8")

        def rebuild() -> None:
            """Redraw the four manifests in the sandbox, as a burn requires.

            A burn is acted on by running the builder; a case that mutates the
            ledger and stops there is testing the un-redrawn manifest, which is
            a different case. Every file the builder writes is in `pristine`, so
            `restore()` takes the sandbox back.
            """
            done = subprocess.run(
                [sys.executable, str(root / BUILDER), "--write"],
                capture_output=True, text=True, check=False, cwd=root)
            if done.returncode != 0:
                raise SystemExit(
                    "FAIL the builder could not redraw the sets in the "
                    f"sandbox:\n{done.stdout}{done.stderr}")

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

        # A tuning document put into a test set: the leak the split exists for.
        case("a tuning document in the test set",
             lambda: swap_in(root, PDFA_TEST, pdfa_tuning["entries"][0]),
             "tuning set")

        # The other capability's test set: one burn would silently spend two.
        case("the other test set's document in this one",
             lambda: swap_in(root, PDFA_TEST, extraction_test["entries"][0]),
             "test set")

        # The same bytes under another name. A name-only check passes this, and
        # it is how the five documents of the 300 in the sealed holdout were
        # found -- so it gets a case whose NAME is unique and whose CONTENT is
        # not, or it would only be re-testing the first case.
        def same_bytes_new_name() -> None:
            borrowed = dict(pdfa_tuning["entries"][0])
            borrowed["name"] = "renamed_but_the_same_bytes.pdf"
            borrowed["key"] = "999/999999.pdf"
            swap_in(root, PDFA_TEST, borrowed)

        case("a tuning document under a different name",
             same_bytes_new_name, "by content")

        def drop_one() -> None:
            document = read(root, PDFA_TEST)
            document["entries"] = document["entries"][1:]
            write(root, PDFA_TEST, document)

        case("a document dropped from the test set", drop_one, "not 300")

        def drop_from_the_300() -> None:
            document = read(root, PDFA_TUNING)
            keep = [e for e in document["entries"]
                    if e["name"] != "000_000012.pdf"]
            document["entries"] = keep
            write(root, PDFA_TUNING, document)

        case("one of the 300 dropped from the tuning set",
             drop_from_the_300, "PDF/A 300")

        # The digest recomputed to match a hand-edited list. This is the case
        # that says why the guard REDRAWS instead of only hashing: a header that
        # agrees with its own entries proves only that whoever edited them could
        # also run sha256.
        def consistent_forgery() -> None:
            sys.path.insert(0, str(root / "scripts" / "ci"))
            for module in ("tuning_and_test_rule",):
                sys.modules.pop(module, None)
            import tuning_and_test_rule as forged_rule  # noqa: PLC0415
            document = read(root, PDFA_TEST)
            document["entries"] = [pdfa_tuning["entries"][0]] + document["entries"][1:]
            document["digest"] = forged_rule.digest_of(document["entries"])
            write(root, PDFA_TEST, document)

        case("a hand-edited set whose digest was recomputed to match",
             consistent_forgery, "not what")

        # A set that is internally perfect and cannot be materialised. The
        # digest still matches, every licence resolves, the split is clean --
        # and the fetcher raises KeyError on 180 of 300 documents.
        def unfetchable_source() -> None:
            document = read(root, PDFA_TEST)
            document["sources"]["govdocs1"].pop("archive_base", None)
            write(root, PDFA_TEST, document)

        case("a source with no archive to fetch from",
             unfetchable_source, "where the archive is")

        def edit_the_ledger() -> None:
            body = pristine[LEDGER].replace(read(root, PDFA_TEST)["digest"],
                                            "0" * 64)
            (root / LEDGER).write_text(body, encoding="utf-8")

        case("the ledger's digest edited", edit_the_ledger, "HOLDOUTS.md")

        # THE BURN PATH (#478). A burn is a fact in the ledger that the draw
        # acts on, and each of the three ways to leave it half-done gets a case:
        # booked and not redrawn, redrawn and not written back, and written back
        # with a name nobody drew.
        burn_row = ("| `{doc}` | pdfa-test300-2026-09-09-v1 | 2026-09-09 | "
                    "{replacement} | a case in "
                    "test_the_tuning_and_test_sets_are_the_drawn_sets.py |\n")

        def book_a_burn(document: str, replacement: str) -> None:
            """Add one row to the ledger's burn table and change nothing else."""
            lines = pristine[LEDGER].splitlines(keepends=True)
            for index, line in enumerate(lines):
                if [cell.strip() for cell in line.strip().strip("|").split("|")] \
                        == list(rule.BURN_COLUMNS):
                    at = index + 2       # past the heading's underline
                    break
            else:
                raise SystemExit(
                    "FAIL the ledger has no burn table with the heading "
                    f"{rule.BURN_COLUMNS}, so the burn cases below test nothing")
            lines.insert(at, burn_row.format(doc=document,
                                             replacement=replacement))
            (root / LEDGER).write_text("".join(lines), encoding="utf-8")

        # A document of the committed test set, burnt in the ledger and left in
        # the file. The redraw no longer holds it; the manifest still does.
        burnt_document = read(root, PDFA_TEST)["entries"][0]["name"]

        case("a burn booked and the set never redrawn",
             lambda: book_a_burn(burnt_document, "`whatever_the_redraw_drew`"),
             "not what")

        # Redrawn but not written back. The manifest is exactly what the rule
        # produces -- an empty `replacement` changes nothing about the draw --
        # so check 1 is green here and only check 7 is not.
        def burn_without_a_replacement() -> None:
            book_a_burn(burnt_document, "")
            rebuild()

        case("a burn whose replacement is not named",
             burn_without_a_replacement, "no replacement named")

        # And a replacement somebody chose rather than one the draw drew.
        def burn_with_an_invented_replacement() -> None:
            book_a_burn(burnt_document, "`a_document_nobody_drew.pdf`")
            rebuild()

        case("a burn replaced by a document of choice",
             burn_with_an_invented_replacement, "not in pdfa-test")

        # And the read itself. Take the burn list back out of the draw and the
        # burnt document returns to the committed set -- which is the whole of
        # what this path buys, stated as a mutation.
        def stop_reading_the_burns() -> None:
            body = pristine[RULE].replace(
                "    rows: list[dict] = []\n    inside = False\n",
                "    rows: list[dict] = []\n    return rows\n    inside = False\n",
                1)
            if body == pristine[RULE]:
                raise SystemExit(
                    "FAIL burns() no longer has the shape this case mutates; "
                    "the case has to be rewritten, not deleted")
            (root / RULE).write_text(body, encoding="utf-8")

        case("the rule no longer reading the burn list",
             stop_reading_the_burns, "not what")

        def change_the_seed() -> None:
            body = pristine[RULE].replace(
                'SEED = "pdfluent-tuning-and-test-2026-09-09"',
                'SEED = "pdfluent-tuning-and-test-quietly-redrawn"')
            (root / RULE).write_text(body, encoding="utf-8")

        case("the seed changed without a redraw", change_the_seed, "seed")

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print("OK test-tuning-sets: the guard is green on the tree and red on all "
          "thirteen ways the split can be broken -- including the same bytes "
          "under a different name, a hand-edited set whose digest was "
          "recomputed to agree with it, a set that is internally perfect and "
          "cannot be fetched, and each of the four ways a burn can be left "
          "half-done")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
