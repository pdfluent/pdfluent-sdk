#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The structured holdout is what its rule produces, on every push (#566).

A sealed set is a promise about the past, and the only thing that can keep it is
a check that runs without anybody remembering to run it. This is that check. It
redraws the split from the committed page list with
`scripts/ci/structured_holdout_rule.py` and refuses a manifest that is not what
the rule produces -- which is what makes the digest in `benchmarks/HOLDOUTS.md`
worth the line it is written on.

The six things it asks, and the failure each one is for:

  1. every page carries the fields a seal is computed over. A manifest missing a
     digest seals nothing, and would seal it quietly.
  2. the split is the rule's split. A page moved from the holdout to the tuning
     half by hand is the whole failure this file exists for, and it leaves no
     other trace.
  3. the two seals are the digests of the two halves.
  4. the halves are disjoint by NAME and by CONTENT. The second is the one that
     costs something to check and the one that catches a real leak: the same
     page under two names is one page in both halves, and no name-based check
     can see it.
  5. `benchmarks/HOLDOUTS.md` carries the holdout's seal. A ledger that does not
     name the digest is a ledger that cannot be checked against later.
  6. the counts in the manifest are the counts of the manifest.

Exit codes:
  0  the manifest is what the rule produces and the ledger names its seal
  1  it is not
  3  cannot check (announced, never silent)
"""
from __future__ import annotations

import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "ci"))

import structured_holdout_rule as rule  # noqa: E402

LEDGER = REPO / "benchmarks" / "HOLDOUTS.md"
REQUIRED = ("source", "name", "document", "sha256", "split")


def main() -> int:
    """Every drawn set, not the first one.

    There are two now (#575): DocLayNet for the five axes and PubTables-1M for
    the table structure, each with its own seed and its own row in the ledger.
    A guard that checked the first would have been green on a second set drawn
    any way at all.
    """
    # EVERY REGISTERED SET, PRESENT OR NOT. Filtering to the files that exist
    # meant deleting one manifest left the guard green on the other: the set
    # was simply not checked, and "I could not look" had quietly become "I
    # looked and it was fine". A registered set with no manifest is exit 3,
    # which is the announced not-a-pass this file already documents. Only a
    # repository that has drawn nothing at all is a repository with nothing to
    # check. (codex, #1930)
    if not any(path.is_file() for path in rule.MANIFESTS.values()):
        print("SKIPPED (not a pass): no structured manifest has been drawn "
              "yet.", file=sys.stderr)
        return 3
    worst = 0
    for name, path in rule.MANIFESTS.items():
        worst = max(worst, check_one(name, path))
    return worst


def check_one(set_name: str, manifest_path: pathlib.Path) -> int:
    if not manifest_path.is_file():
        # NOT a pass, and not a failure either. The manifest is written by the
        # run that draws the set, and a repository that has not drawn one yet is
        # a repository with nothing to check rather than a broken one. Exit 3
        # says so out loud, which is the house rule for "could not check".
        print(f"SKIPPED (not a pass): {manifest_path.relative_to(REPO)} is not "
              "there, so this set has not been drawn yet.", file=sys.stderr)
        return 3

    seed = rule.seed_for(set_name)
    try:
        manifest = json.loads(manifest_path.read_text())
    except json.JSONDecodeError as exc:
        print(f"{manifest_path.relative_to(REPO)} is not readable JSON: {exc}",
              file=sys.stderr)
        return 1

    problems: list[str] = []
    pages = manifest.get("pages")
    if not isinstance(pages, list) or not pages:
        print(f"{manifest_path.relative_to(REPO)} names no pages, so it seals "
              "nothing.", file=sys.stderr)
        return 1

    # 1. the fields a seal is computed over ---------------------------------
    for i, page in enumerate(pages):
        missing = [f for f in REQUIRED if not page.get(f)]
        if missing:
            problems.append(
                f"page {i} ({page.get('name', 'unnamed')}) is missing "
                f"{', '.join(missing)}. A seal over a manifest with holes seals "
                "the holes too.")
    if problems:
        return _report(problems)

    # 2. the split is the rule's split --------------------------------------
    tuning, holdout = rule.split(pages, seed=seed)
    want_holdout = {p["name"] for p in holdout}
    have_holdout = {p["name"] for p in pages if p["split"] == "holdout"}
    have_tuning = {p["name"] for p in pages if p["split"] == "tuning"}
    unknown = {p["split"] for p in pages} - {"tuning", "holdout"}
    if unknown:
        problems.append(
            f"{len(unknown)} page(s) carry a split that is neither `tuning` nor "
            f"`holdout`: {', '.join(sorted(unknown))}.")
    if have_holdout != want_holdout:
        moved_in = sorted(have_holdout - want_holdout)
        moved_out = sorted(want_holdout - have_holdout)
        problems.append(
            f"the recorded split is not the one {seed} produces: "
            f"{len(moved_in)} page(s) marked holdout that the rule puts in the "
            f"tuning half, {len(moved_out)} the other way round. "
            f"Redraw with `python3 scripts/ci/loop/structured_bench.py "
            f"--set {manifest.get('set', set_name)} --draw`, or say on the "
            "ticket why the rule changed.")
    if have_tuning != {p["name"] for p in tuning}:
        problems.append("the tuning half is not the complement of the holdout.")

    # 3. the seals -----------------------------------------------------------
    seal = manifest.get("seal", {})
    for half, rows in (("holdout", holdout), ("tuning", tuning)):
        want = rule.digest_of(rows)
        have = seal.get(half)
        if have != want:
            problems.append(
                f"the {half} seal in the manifest is {have!r}; the digest of "
                f"what it names is {want}.")

    # 4. disjoint by name AND by content ------------------------------------
    both = have_holdout & have_tuning
    if both:
        problems.append(f"{len(both)} page name(s) are in both halves: "
                        f"{', '.join(sorted(both)[:5])}.")
    holdout_bytes = {p["sha256"] for p in pages if p["split"] == "holdout"}
    tuning_bytes = {p["sha256"] for p in pages if p["split"] == "tuning"}
    shared = holdout_bytes & tuning_bytes
    if shared:
        problems.append(
            f"{len(shared)} page(s) are in both halves BY CONTENT under "
            "different names. That is a leak no name check can see, and it "
            "spends the holdout: the rounds tune on bytes that are also sealed.")

    # 5. the ledger names the seal ------------------------------------------
    if not LEDGER.is_file():
        problems.append(f"{LEDGER.relative_to(REPO)} is not there, so no ledger "
                        "records this draw.")
    else:
        text = LEDGER.read_text()
        if seal.get("holdout") and seal["holdout"] not in text:
            problems.append(
                f"{LEDGER.relative_to(REPO)} does not carry the holdout seal "
                f"{seal['holdout'][:16]}…. A digest published after the numbers "
                "establishes nothing, which is the whole reason the ledger "
                "exists.")

    # 6. the counts are the counts ------------------------------------------
    counts = manifest.get("counts", {})
    for half, rows in (("holdout", holdout), ("tuning", tuning)):
        if counts.get(half) is not None and counts[half] != len(rows):
            problems.append(f"the manifest says {counts[half]} {half} page(s) "
                            f"and names {len(rows)}.")

    if problems:
        return _report(problems)

    print(f"✓ structured holdout `{set_name}`: {len(holdout)} page(s) sealed "
          f"of {len(pages)} ({100 * len(holdout) / len(pages):.1f}%), the split "
          f"is what {seed} produces, the halves are disjoint by name and by "
          "content, and the ledger names the seal")
    return 0


def _report(problems: list[str]) -> int:
    print(f"The structured holdout: {len(problems)} problem(s)\n", file=sys.stderr)
    for p in problems:
        print(f"  - {p}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
