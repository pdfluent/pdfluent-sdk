#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Draw the four sets of #461 and write the four manifests.

    python3 scripts/ci/build_tuning_and_test_manifests.py --write
    python3 scripts/ci/build_tuning_and_test_manifests.py --check   # no writes

It reads only files that are already in the repository -- the ten thousand of
#377 as two entry tables, the PDF/A 300, and the pinned extraction sets -- and
fetches nothing. The four manifests are therefore reproducible on any machine,
which is what `the_tuning_and_test_sets_are_the_drawn_sets.py` leans on when it
redraws them on every push.

THE FOUR SETS

  corpus/pdfa/PDFA_TUNING_1000.json         the 300 the rounds already stand on,
                                            plus 700 drawn from the tuning half,
                                            apportioned so the thousand is six
                                            parts GovDocs1 to four SAFEDOCS.
  corpus/pdfa/PDFA_TEST_300.json            300 from the holdout half, disjoint
                                            from all four tuning inputs.
  corpus/extraction/EXTRACTION_TUNING_1000.json
                                            the whole pinned gate set (500, and
                                            with it the 121 of the comparison),
                                            the govdocs 300, and 200 more from
                                            the tuning half.
  corpus/extraction/EXTRACTION_TEST_300.json
                                            300 from the holdout half, disjoint
                                            from the PDF/A test set as well.

WHY THE EXTRACTION TEST SET IS NOT SHAPED LIKE ITS TUNING SET

Half of the extraction tuning set is test-suite material -- documents that once
broke a reader, many with no text layer at all -- and there is no second pinned
suite to draw a test set of the same shape from. So the test set is ordinary
government paperwork, and its absolute recall will sit above the tuning set's
for reasons that have nothing to do with the code. That is why the overfitting
check in `scripts/ci/loop/overfitting_gate.py` compares DIRECTIONS between two
rounds and never levels between the two sets: the one comparison the shapes
support is "this axis went up here and down there".
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import tuning_and_test_rule as rule  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
PDFA_CORPUS = REPO / "corpus" / "pdfa"
EXTRACTION_CORPUS = REPO / "corpus" / "extraction"
PDFA_300 = REPO / "crates" / "pdf-manip" / "examples" / "pdfa_fidelity_inputs_300.json"
EXTRACTION_PINNED = EXTRACTION_CORPUS / "tuning.json"

SOURCE_LETTER = {"g": "govdocs1", "s": "safedocs"}
SCHEMA = "pdfluent.tuning-and-test/1"

OUTPUTS = {
    "pdfa-tuning": PDFA_CORPUS / "PDFA_TUNING_1000.json",
    "pdfa-test": PDFA_CORPUS / "PDFA_TEST_300.json",
    "extraction-tuning": EXTRACTION_CORPUS / "EXTRACTION_TUNING_1000.json",
    "extraction-test": EXTRACTION_CORPUS / "EXTRACTION_TEST_300.json",
}


def flat_name(source: str, key: str) -> str:
    """The name a document carries on disk, as `fetch_pdfa_corpus.py` writes it."""
    return key.replace("/", "_") if source == "govdocs1" else key


def read_json(path: pathlib.Path):
    if not path.is_file():
        raise SystemExit(f"[tuning-sets] FATAL: {path} is missing")
    return json.loads(path.read_text(encoding="utf-8"))


def pdfa_half(half: str) -> list[dict]:
    """One half of the ten thousand, as rows this rule can draw from."""
    rows: list[dict] = []
    for path in sorted(PDFA_CORPUS.glob(f"pdfa_{half}_entries.*.tsv")):
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line:
                continue
            parts = line.split("\t")
            source = SOURCE_LETTER[parts[0]]
            rows.append({
                "source": source,
                "key": parts[1],
                "name": flat_name(source, parts[1]),
                "sha256": parts[2],
                "bytes": int(parts[3]),
                "zip": parts[4],
                "offset": int(parts[5]),
                "method": int(parts[6]),
                "csize": int(parts[7]),
            })
    if not rows:
        raise SystemExit(f"[tuning-sets] FATAL: no entries for the {half} half")
    return rows


def pdfa_300() -> list[dict]:
    """The 300 the published PDF/A series is measured on.

    They carry a name and a digest and no archive coordinates: the set predates
    the corpus rule and is materialised by unpacking whole GovDocs1 zips
    (`scripts/corpus-download-govdocs.sh`), which is what the wave already does.
    The zip is derived from the name rather than invented, so a document whose
    name does not carry one is a loud failure and not a silent guess.
    """
    rows = []
    for entry in read_json(PDFA_300):
        name = entry["file"]
        stem = name[:-4] if name.endswith(".pdf") else name
        if "_" not in stem:
            raise SystemExit(
                f"[tuning-sets] FATAL: {name} in {PDFA_300.name} is not a "
                "GovDocs1 name of the form <zip>_<number>.pdf, so the archive "
                "it has to be fetched from cannot be derived")
        zip_id, number = stem.split("_", 1)
        rows.append({
            "source": "govdocs1",
            "key": f"{zip_id}/{number}.pdf",
            "name": name,
            "sha256": entry["sha256"],
            "zip": zip_id,
        })
    return rows


def extraction_set(name: str) -> list[dict]:
    """One pinned extraction set, as rows, straight out of its manifest."""
    manifest = read_json(EXTRACTION_PINNED)
    block = manifest.get("sets", {}).get(name)
    if not block:
        raise SystemExit(
            f"[tuning-sets] FATAL: {EXTRACTION_PINNED.name} has no set {name!r}")
    rows = []
    for entry in block["entries"]:
        row = {
            "source": entry.get("source", "govdocs1"),
            "key": entry.get("path") or entry["name"],
            "name": entry["name"],
            "sha256": entry["sha256"],
        }
        if entry.get("path"):
            row["path"] = entry["path"]
        if entry.get("bytes"):
            row["bytes"] = entry["bytes"]
        rows.append(row)
    return rows


def coordinates_from(half: list[dict], rows: list[dict]) -> list[dict]:
    """Give a row the archive coordinates of the same bytes, when the half has them.

    The 300 and the eleven of them that also sit in the tuning half are the same
    documents; naming them twice with two different fetch routes would make the
    manifest say two things about one file.
    """
    by_content = {row["sha256"]: row for row in half}
    out = []
    for row in rows:
        known = by_content.get(row["sha256"])
        merged = dict(row)
        if known:
            for field in ("bytes", "zip", "offset", "method", "csize"):
                merged.setdefault(field, known[field])
        out.append(merged)
    return out


def unique(rows: list[dict]) -> tuple[list[dict], list[str]]:
    """Rows with no name and no digest repeated, and what was dropped."""
    names: set[str] = set()
    content: set[str] = set()
    kept: list[dict] = []
    dropped: list[str] = []
    for row in rows:
        if row["name"] in names or row["sha256"] in content:
            dropped.append(row["name"])
            continue
        names.add(row["name"])
        content.add(row["sha256"])
        kept.append(row)
    return kept, dropped


def entries_of(rows: list[dict]) -> list[dict]:
    """Manifest order: by name, so a diff between two draws is readable."""
    return [dict(sorted(row.items())) for row in sorted(rows, key=lambda r: r["name"])]


def sources_for(rows: list[dict]) -> dict:
    """The provenance block, copied from the manifests the rows came out of.

    Copied and not summarised: `ratchet.licence_of` resolves a document's licence
    through this block, and a set whose entries cannot name a licence is a set
    that cannot be redistributed with the measurement (#215).
    """
    pdfa = read_json(PDFA_CORPUS / "PDFA_TUNING_MANIFEST.json")["sources"]
    extraction = read_json(EXTRACTION_PINNED)["sets"]["gate"]["sources"]
    govdocs_note = read_json(EXTRACTION_PINNED)["sets"]["govdocs"]["sources"]

    # Merged per source and never replaced wholesale. Both files describe
    # `govdocs1`, and only the PDF/A one carries `archive_base` -- the extraction
    # manifest's is a provenance note, because that set is materialised by
    # unpacking zips rather than by ranged request. A plain `{**a, **b}` let the
    # note win, and the fetcher then raised KeyError('govdocs1') on all 180
    # GovDocs1 documents of the test set while the 120 SAFEDOCS ones arrived
    # fine (run 34292223960). The archive-bearing definition wins its own keys;
    # the check in the guard is what makes that stay true.
    known: dict[str, dict] = {}
    for block in (govdocs_note, extraction, pdfa):
        for name, source in block.items():
            known.setdefault(name, {}).update(source)
    used = sorted({row["source"] for row in rows})
    missing = [name for name in used if name not in known]
    if missing:
        raise SystemExit(
            f"[tuning-sets] FATAL: no provenance for source(s) {missing}; a "
            "document that cannot name where it came from does not go in a "
            "manifest (#215)")
    return {name: known[name] for name in used}


def build() -> dict[str, dict]:
    """The four sets, drawn in the one order that makes them disjoint."""
    tuning_half = pdfa_half("tuning")
    holdout_half = pdfa_half("holdout")

    # PDF/A tuning: the 300 first, because they are the reason the baselines
    # carry over, and then the rest of the thousand from the tuning half.
    forced_pdfa, dropped_pdfa = unique(coordinates_from(tuning_half, pdfa_300()))
    names = {row["name"] for row in forced_pdfa}
    content = {row["sha256"] for row in forced_pdfa}
    room = rule.quota(rule.TUNING_SIZE, rule.SOURCE_SHARE)
    for row in forced_pdfa:
        room[row["source"]] = room.get(row["source"], 0) - 1
    if any(value < 0 for value in room.values()):
        raise SystemExit(
            "[tuning-sets] FATAL: the forced documents already exceed the "
            f"quota for their source: {room}")
    pdfa_tuning = forced_pdfa + rule.fill(
        tuning_half, "pdfa-tuning", room, names, content)

    # Extraction tuning: the whole gate set (the 121 comparison documents are
    # inside it), the govdocs 300, and the rest from the same tuning half.
    forced_extraction, dropped_extraction = unique(
        coordinates_from(tuning_half, extraction_set("gate") + extraction_set("govdocs")))
    e_names = {row["name"] for row in forced_extraction}
    e_content = {row["sha256"] for row in forced_extraction}
    e_room = rule.quota(
        rule.TUNING_SIZE - len(forced_extraction), rule.SOURCE_SHARE)
    extraction_tuning = forced_extraction + rule.fill(
        tuning_half, "extraction-tuning", e_room, e_names, e_content)

    # Both test sets: out of the holdout half, and out of what neither tuning
    # set nor the ten thousand already holds. The five documents of the 300 that
    # sit in the sealed holdout are excluded here, by content, and that is the
    # whole reason the exclusion is by content.
    spent_names = {row["name"] for row in pdfa_tuning + extraction_tuning}
    spent_names |= {row["name"] for row in tuning_half}
    spent_content = {row["sha256"] for row in pdfa_tuning + extraction_tuning}
    spent_content |= {row["sha256"] for row in tuning_half}

    # AND OUT OF WHAT HAS BEEN LOOKED AT. A burnt document is spent in exactly
    # the sense the two lines above mean it, so it joins them here rather than
    # being handled somewhere of its own: the walk then goes one document
    # further and that document is the replacement (#478).
    #
    # By content as well as by name, for the reason the five are: the holdout
    # half holds the same bytes under more than one name, and a burn that only
    # took the name out would leave the document itself in the set.
    burnt = rule.burns()
    known = {row["name"]: row for row in tuning_half + holdout_half}
    unknown = sorted({b["document"] for b in burnt} - set(known))
    if unknown:
        raise SystemExit(
            "[tuning-sets] FATAL: benchmarks/HOLDOUTS.md books a burn for "
            f"{', '.join(unknown)}, which is in neither half of the ten "
            "thousand. A burn the draw cannot find is a burn that does not "
            "happen, which is the failure #478 was opened for -- check the "
            "spelling of the name against the manifest.")
    for burn in burnt:
        spent_names.add(burn["document"])
        spent_content.add(known[burn["document"]]["sha256"])

    test_room = rule.quota(rule.TEST_SIZE, rule.SOURCE_SHARE)
    pdfa_test = rule.fill(
        holdout_half, "pdfa-test", dict(test_room), spent_names, spent_content)
    extraction_test = rule.fill(
        holdout_half, "extraction-test", dict(test_room), spent_names, spent_content)

    drawn = {
        "pdfa-tuning": (pdfa_tuning, rule.TUNING_SIZE),
        "pdfa-test": (pdfa_test, rule.TEST_SIZE),
        "extraction-tuning": (extraction_tuning, rule.TUNING_SIZE),
        "extraction-test": (extraction_test, rule.TEST_SIZE),
    }
    for label, (rows, want) in drawn.items():
        if len(rows) != want:
            raise SystemExit(
                f"[tuning-sets] FATAL: {label} drew {len(rows)} of {want}. The "
                "pool ran out; a short set is a smaller denominator on every "
                "axis and is never padded from elsewhere.")

    return {
        "pdfa-tuning": manifest(
            "pdfa", "tuning", "tuning1000", pdfa_tuning,
            forced="crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json",
            populations=["corpus/pdfa/PDFA_TUNING_MANIFEST.json",
                         "crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json"],
            duplicates=dropped_pdfa),
        "pdfa-test": manifest(
            "pdfa", "test", "test300", pdfa_test, forced=None,
            populations=["corpus/pdfa/PDFA_HOLDOUT_MANIFEST.json"],
            duplicates=[]),
        "extraction-tuning": manifest(
            "extraction", "tuning", "tuning1000", extraction_tuning,
            forced="corpus/extraction/tuning.json (sets gate + govdocs)",
            populations=["corpus/extraction/tuning.json",
                         "corpus/pdfa/PDFA_TUNING_MANIFEST.json"],
            duplicates=dropped_extraction),
        "extraction-test": manifest(
            "extraction", "test", "test300", extraction_test, forced=None,
            populations=["corpus/pdfa/PDFA_HOLDOUT_MANIFEST.json"],
            duplicates=[]),
    }


def manifest(capability: str, role: str, set_name: str, rows: list[dict],
             forced: str | None, populations: list[str],
             duplicates: list[str]) -> dict:
    entries = entries_of(rows)
    composition: dict[str, int] = {}
    for entry in entries:
        composition[entry["source"]] = composition.get(entry["source"], 0) + 1
    # The burns of THIS set, copied out of the ledger so the file says what it
    # has been through. The ledger stays the source: this block is written by
    # the draw and checked against the ledger by the guard, never edited (#478).
    draw = rule.draw_name(capability, set_name)
    burnt = [{"document": burn["document"], "date": burn["date"],
              "replacement": burn["replacement"], "reason": burn["reason"]}
             for burn in rule.burns() if burn["set"] == draw]
    return {
        "schema": SCHEMA,
        "capability": capability,
        "role": role,
        "set": set_name,
        "draw": draw,
        "burnt_and_replaced": burnt,
        "description": DESCRIPTIONS[f"{capability}-{role}"],
        "generator": "scripts/ci/build_tuning_and_test_manifests.py",
        "rule": {
            "module": "scripts/ci/tuning_and_test_rule.py",
            "seed": rule.SEED,
            "drawn": rule.DRAWN,
            "version": rule.RULE_VERSION,
            "source_share": rule.SOURCE_SHARE,
            "label": f"{capability}-{role}",
        },
        "populations": populations,
        "forced_subset": forced,
        "content_duplicates_dropped": sorted(duplicates),
        "composition": dict(sorted(composition.items())),
        "total": len(entries),
        "digest": rule.digest_of(entries),
        "materialise": MATERIALISE[capability],
        "sources": sources_for(rows),
        "entries": entries,
    }


DESCRIPTIONS = {
    "pdfa-tuning": (
        "The PDF/A tuning set (#461): a thousand documents that debugging, "
        "classification and fixes may look at. Contains the 300 of the "
        "published series as a subset, so benchmarks/pdfa/baseline.json carries "
        "over rather than being redrawn."),
    "pdfa-test": (
        "The PDF/A test set (#461): three hundred documents held out of every "
        "tuning set and out of the ten thousand of #377. Run at the end of a "
        "round, reported as aggregates per axis and as the count of documents "
        "under each threshold. A document opened to explain a failure is burnt: "
        "it is booked in benchmarks/HOLDOUTS.md, the draw excludes it from then "
        "on, and the seeded walk goes one document further -- that document is "
        "the replacement, and the ledger names it."),
    "extraction-tuning": (
        "The text-extraction tuning set (#461): the whole pinned gate set -- "
        "and with it the 121 documents of the #390 comparison -- plus the "
        "govdocs 300 and two hundred more from the #377 tuning half."),
    "extraction-test": (
        "The text-extraction test set (#461): three hundred documents held out "
        "of every tuning set, of the ten thousand, and of the PDF/A test set. "
        "Ordinary government paperwork, where the tuning set is half test-suite "
        "material, so its absolute recall is not comparable with the tuning "
        "set's -- only the direction between two rounds is."),
}

MATERIALISE = {
    "pdfa": (
        "python3 scripts/ci/fetch_pdfa_corpus.py --set <this file> --out <dir>. "
        "Entries with archive coordinates are single ranged requests; the 300 "
        "carry none and come from scripts/corpus-download-govdocs.sh --zips "
        "\"$(python3 scripts/pdfa/the_300_are_the_pinned_300.py --print-zips)\" "
        "into the same directory first."),
    "extraction": (
        "python3 scripts/ci/fetch_gate_corpus.py --manifest "
        "corpus/GATE_CORPUS_MANIFEST.json --out <dir>, then "
        "scripts/corpus-download-govdocs.sh for the 300, then "
        "python3 scripts/ci/fetch_pdfa_corpus.py --set <this file> --out <dir> "
        "for the rest. One directory: the three name spaces do not collide."),
}


def render(document: dict) -> str:
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


def main() -> int:
    p = argparse.ArgumentParser(description="Draw the four sets of #461.")
    p.add_argument("--write", action="store_true",
                   help="write the four manifests; without it nothing is written")
    p.add_argument("--check", action="store_true",
                   help="exit 1 if what is on disk is not what the rule draws")
    args = p.parse_args()

    built = build()
    changed = []
    for label, document in built.items():
        path = OUTPUTS[label]
        body = render(document)
        if args.write:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body, encoding="utf-8")
        current = path.read_text(encoding="utf-8") if path.is_file() else None
        if current != body:
            changed.append(path.relative_to(REPO).as_posix())
        print(f"[tuning-sets] {label}: {document['total']} documents, "
              f"{document['composition']}, digest {document['digest'][:16]}…")

    if args.write:
        print(f"[tuning-sets] wrote {len(built)} manifest(s)")
        return 0
    if args.check and changed:
        print("[tuning-sets] FATAL: these files are not what the rule draws: "
              + ", ".join(changed), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
