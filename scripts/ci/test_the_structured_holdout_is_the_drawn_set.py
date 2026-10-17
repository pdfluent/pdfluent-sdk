#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Prove the structured holdout guard is red on each way the seal can be spoiled (#566).

A guard that has never been red has never been tested, and this one carries the
whole value of the release measurement of #574: that the pages behind it are
pages nobody looked at while the rounds were running. So every way that promise
can be broken gets a case, and each case is a mutation of a manifest the rule
itself produced.

The manifest is BUILT here rather than copied from the repository. Copying would
tie this test to whichever set had last been drawn -- green on a tree with no
manifest, red the day the corpus changed for reasons that have nothing to do
with the guard -- and it would not test the case that matters most, which is a
manifest somebody edited by hand.

  a page moved into the tuning half        the leak the seal exists for, and the
                                           one no other check can see.
  a page in both halves by name            a split that does not add up.
  the same BYTES under two names           the leak a name check misses. It is
                                           not hypothetical: five documents of
                                           the PDF/A 300 turned out to sit in a
                                           sealed half under their own names.
  a seal edited to agree with a hand
    edit                                   the redraw, and not the digest, is
                                           what makes a manifest checkable.
  the ledger no longer names the seal      a digest published after the numbers
                                           establishes nothing.
  a page with no digest                    a seal over a manifest with holes
                                           seals the holes.
  the counts edited                        a manifest that says it measured more
                                           pages than it names.
  a split value that is neither half       a third category nobody defined is a
                                           page nobody counted.

Exit codes:
  0  the guard is green on a manifest the rule produced and red on every mutation
  1  it is not
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = "scripts/ci/the_structured_holdout_is_the_drawn_set.py"
RULE = "scripts/ci/structured_holdout_rule.py"
MANIFEST = "corpus/structured/DOCLAYNET_TEST.json"
LEDGER = "benchmarks/HOLDOUTS.md"

sys.path.insert(0, str(REPO / "scripts" / "ci"))
import structured_holdout_rule as rule  # noqa: E402


def pages(count: int = 120, per_document: int = 4) -> list[dict]:
    """A set with the shape the real one has: several pages per document."""
    out = []
    for i in range(count):
        out.append({
            "source": "doclaynet-v1.2/test",
            "name": f"page{i:04d}.pdf",
            "document": f"document{i // per_document:03d}.pdf",
            "page_no": i % per_document,
            "category": "manuals",
            "sha256": hashlib.sha256(f"page{i}".encode()).hexdigest(),
        })
    return out


def manifest_of(rows: list[dict], set_name: str = "doclaynet") -> dict:
    seed = rule.seed_for(set_name)
    tuning, holdout = rule.split(rows, seed=seed)
    held = {p["name"] for p in holdout}
    entries = [dict(p, split=("holdout" if p["name"] in held else "tuning")) for p in rows]
    return {
        "set": set_name,
        "version": "test fixture",
        "drawn": "2026-09-17",
        "seed": seed,
        "rule": RULE,
        "share": rule.HOLDOUT_SHARE,
        "counts": {"pages": len(entries), "tuning": len(tuning),
                   "holdout": len(holdout),
                   "documents": len(rule.documents_of(entries))},
        "seal": {"tuning": rule.digest_of(tuning),
                 "holdout": rule.digest_of(holdout)},
        "pages": entries,
    }


def build_sandbox(root: pathlib.Path, document: dict) -> None:
    for name in (GUARD, RULE):
        target = root / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO / name, target)
    (root / MANIFEST).parent.mkdir(parents=True, exist_ok=True)
    write(root, document)
    # EVERY REGISTERED SET GETS ONE. The guard iterates the sets the rule
    # names, present or not, so a sandbox holding one of two would be red for
    # a reason that has nothing to do with the case being driven -- and the
    # mutations below would then prove nothing. (codex, #1930)
    seals = [document["seal"]["holdout"]]
    for set_name, path in rule.MANIFESTS.items():
        if path == rule.MANIFEST:
            continue
        other = manifest_of(pages(count=40, per_document=2), set_name)
        place = root / path.relative_to(REPO)
        place.parent.mkdir(parents=True, exist_ok=True)
        place.write_text(json.dumps(other, indent=1) + "\n")
        seals.append(other["seal"]["holdout"])
    (root / LEDGER).parent.mkdir(parents=True, exist_ok=True)
    (root / LEDGER).write_text(
        "# Holdout ledger\n\n| draw | seal |\n|---|---|\n"
        + "".join(f"| structured-fixture | `{seal}` |\n" for seal in seals))


def write(root: pathlib.Path, document: dict) -> None:
    (root / MANIFEST).write_text(json.dumps(document, indent=1) + "\n")


def run(root: pathlib.Path) -> tuple[int, str]:
    done = subprocess.run([sys.executable, str(root / GUARD)],
                          capture_output=True, text=True, check=False, cwd=root)
    return done.returncode, done.stdout + done.stderr


def a_registered_set_with_no_manifest_is_not_a_pass() -> str | None:
    """Deleting one manifest must not leave the guard green on the other.

    The guard iterated the manifests that EXIST, so a registered, referenced
    set whose file was gone was simply not checked -- "I could not look" had
    become "I looked and it was fine", which is the distinction this file's
    own header is about. (codex, #1930)
    """
    text = (REPO / GUARD).read_text()
    if "if path.is_file()" in text.split("def main", 1)[-1].split("def ", 2)[0]:
        return ("the guard filters the registered manifests to the ones that "
                "exist, so a missing second seal reduces its coverage to one "
                "set instead of returning the announced not-a-pass.")
    return None


def main() -> int:
    failures: list[str] = []
    coverage = a_registered_set_with_no_manifest_is_not_a_pass()
    if coverage:
        failures.append(coverage)
    rows = pages()
    pristine = manifest_of(rows)

    with tempfile.TemporaryDirectory(prefix="structured-holdout-test-") as tmp:
        root = pathlib.Path(tmp)
        build_sandbox(root, pristine)

        code, output = run(root)
        if code != 0:
            print("FAIL the guard is red on a manifest its own rule produced, so "
                  f"nothing below proves anything:\n{output}")
            return 1
        if len([p for p in pristine["pages"] if p["split"] == "holdout"]) == 0:
            print("FAIL the fixture drew an empty holdout, so every case below "
                  "would pass for the wrong reason")
            return 1

        ledger = (root / LEDGER).read_text()

        def case(what: str, mutate, expect: str) -> None:
            document = json.loads(json.dumps(pristine))
            mutate(document)
            write(root, document)
            code, output = run(root)
            if code == 0:
                failures.append(f"{what}: the guard stayed green")
            elif expect not in output:
                failures.append(f"{what}: red, but not about {expect!r}:\n{output}")
            write(root, pristine)
            (root / LEDGER).write_text(ledger)

        def first_holdout(document: dict) -> dict:
            return next(p for p in document["pages"] if p["split"] == "holdout")

        # 1. THE LEAK. One sealed page quietly marked tuning: the rounds may now
        #    look at it, and every other field still agrees with itself.
        def move_one(document: dict) -> None:
            first_holdout(document)["split"] = "tuning"
        case("a sealed page moved into the tuning half", move_one,
             "is not the one")

        # 2. A page on both sides.
        def duplicate(document: dict) -> None:
            page = dict(first_holdout(document), split="tuning")
            document["pages"].append(page)
        case("a page in both halves by name", duplicate, "")

        # 3. THE CONTENT LEAK. A tuning page given the bytes of a sealed one.
        def same_bytes(document: dict) -> None:
            sealed = first_holdout(document)
            tuning = next(p for p in document["pages"] if p["split"] == "tuning")
            tuning["sha256"] = sealed["sha256"]
        case("the same bytes in both halves under two names", same_bytes,
             "BY CONTENT")

        # 4. A seal recomputed to agree with a hand edit. The point of the
        #    redraw: agreeing with yourself is not evidence.
        def reseal(document: dict) -> None:
            first_holdout(document)["split"] = "tuning"
            tuning = [p for p in document["pages"] if p["split"] == "tuning"]
            holdout = [p for p in document["pages"] if p["split"] == "holdout"]
            document["seal"] = {"tuning": rule.digest_of(tuning),
                                "holdout": rule.digest_of(holdout)}
            document["counts"]["tuning"] = len(tuning)
            document["counts"]["holdout"] = len(holdout)
        case("a hand edit with the seals recomputed to match", reseal,
             "is not the one")

        # 5. The ledger stops naming the seal.
        def blank_ledger(document: dict) -> None:
            (root / LEDGER).write_text("# Holdout ledger\n\nnothing here\n")
        case("the ledger no longer names the seal", blank_ledger, "does not carry")

        # 6. A page with no digest.
        def no_digest(document: dict) -> None:
            first_holdout(document)["sha256"] = ""
        case("a page with no digest", no_digest, "missing sha256")

        # 7. Counts that overstate the set.
        def wrong_count(document: dict) -> None:
            document["counts"]["holdout"] = document["counts"]["holdout"] + 7
        case("the counts edited", wrong_count, "and names")

        # 8. A third category.
        def third_split(document: dict) -> None:
            first_holdout(document)["split"] = "maybe"
        case("a split that is neither half", third_split, "neither")

    for line in failures:
        print(f"FAIL {line}")
    if failures:
        print(f"\n{len(failures)} of the structured holdout's guarantees are "
              "not guarded.")
        return 1
    print("✓ the structured holdout guard is red on all eight ways the seal "
          "can be spoiled, including the two leaks a name check cannot see")
    return 0


if __name__ == "__main__":
    sys.exit(main())
