#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Build the pinned corpus manifest for the text-extraction capability (#367).

The manifest is assembled from sets that are already pinned elsewhere in this
tree rather than drawn afresh, so that the extraction rig measures the same
bytes the PDF/A rig does and a disagreement between the two capabilities is a
real disagreement rather than two different corpora:

  * ``gate``    -- the 500 test-suite documents of ``corpus/GATE_CORPUS_MANIFEST.json``
                   (pdf.js, PDFBox, veraPDF; Apache-2.0 and CC-BY-4.0). Fetched
                   by URL at a pinned upstream commit, never committed here (#215).
  * ``govdocs`` -- the 300 GovDocs1 documents of
                   ``crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json``,
                   the PDF/A tuning set. Public-domain US government documents;
                   they live on the corpus host, not in this repository.

Why two sets and not one. The gate corpus is testsuite material: largely
documents that once broke something, many without a text layer at all, which is
exactly what A27 says about its own tail. GovDocs is ordinary government
paperwork. A recall figure means a different thing on each, so they are measured
and ratcheted apart and never averaged into one number.

Usage:
    python3 scripts/extraction/build_extraction_manifest.py \
        --out corpus/extraction/tuning.json
    python3 scripts/extraction/build_extraction_manifest.py --self-test
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


REPO = pathlib.Path(__file__).resolve().parents[2]
GATE_MANIFEST = REPO / "corpus" / "GATE_CORPUS_MANIFEST.json"
GOVDOCS_SHAS = REPO / "crates" / "pdf-manip" / "examples" / "pdfa_fidelity_inputs_300.json"

MANIFEST_VERSION = 1

# Floors, in the two-way shape gate_corpus_is_pinned.py uses: a set that shrinks
# measures less while still reporting green, and a set that grows on its own has
# a baseline nobody chose.
EXPECTED = {"gate": 500, "govdocs": 300}


def read_json(path: pathlib.Path) -> object:
    with path.open(encoding="utf-8") as f:
        return json.load(f)


def gate_set(gate_manifest: dict) -> dict:
    """The gate corpus, carried over entry for entry with its own provenance."""
    entries = [
        {
            "name": e["name"],
            "sha256": e["sha256"],
            "bytes": e["bytes"],
            "source": e["source"],
            "path": e["path"],
        }
        for e in gate_manifest["entries"]
    ]
    return {
        "description": (
            "Test-suite documents from pdf.js, PDFBox and the veraPDF corpus. "
            "Largely documents that once broke a reader: many carry no text "
            "layer, some are damaged by design. A recall number on this set is "
            "a statement about hard cases, not about ordinary documents."
        ),
        "materialise": (
            "python3 scripts/ci/fetch_gate_corpus.py "
            "--manifest corpus/GATE_CORPUS_MANIFEST.json --out <dir>"
        ),
        "inherits": "corpus/GATE_CORPUS_MANIFEST.json",
        "sources": gate_manifest["sources"],
        "entries": sorted(entries, key=lambda e: e["name"]),
    }


def govdocs_set(shas: list) -> dict:
    """The PDF/A tuning set, by name and digest; the bytes live on the host."""
    entries = [{"name": e["file"], "sha256": e["sha256"]} for e in shas]
    return {
        "description": (
            "The 300-document GovDocs1 tuning set the PDF/A rounds are measured "
            "on. Ordinary government paperwork with real text layers, which is "
            "why the extraction axes mean something different here than on the "
            "gate set."
        ),
        "materialise": (
            "Present on the corpus host under the govdocs corpus directory; "
            "point --corpus-dir at it. Not fetchable from this repository (#215)."
        ),
        "inherits": "crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json",
        "sources": {
            "govdocs1": {
                "repo": "digitalcorpora GovDocs1",
                "licence": "public-domain-usgov",
                "provenance": (
                    "US government documents from the GovDocs1 corpus, "
                    "s3://digitalcorpora. 83 of the 300 are byte-identical to "
                    "the public 000.zip (A29)."
                ),
            }
        },
        "entries": sorted(entries, key=lambda e: e["name"]),
    }


def build() -> dict:
    gate = gate_set(read_json(GATE_MANIFEST))
    gov = govdocs_set(read_json(GOVDOCS_SHAS))
    return {
        "manifest_version": MANIFEST_VERSION,
        "capability": "extraction",
        "description": (
            "Pinned corpus for the text-extraction capability (#367). Two sets, "
            "measured and ratcheted separately, never averaged together."
        ),
        "generator": "scripts/extraction/build_extraction_manifest.py",
        "sets": {"gate": gate, "govdocs": gov},
    }


def check(manifest: dict) -> list:
    """Everything a reader of this manifest is entitled to assume."""
    problems = []
    if manifest.get("manifest_version") != MANIFEST_VERSION:
        problems.append(f"manifest_version is not {MANIFEST_VERSION}")
    sets = manifest.get("sets", {})
    if set(sets) != set(EXPECTED):
        problems.append(f"sets are {sorted(sets)}, expected {sorted(EXPECTED)}")
    for name, want in EXPECTED.items():
        s = sets.get(name)
        if not s:
            continue
        entries = s.get("entries", [])
        if len(entries) != want:
            problems.append(f"set {name}: {len(entries)} entries, expected exactly {want}")
        seen = set()
        for e in entries:
            if not e.get("name"):
                problems.append(f"set {name}: an entry has no name")
            digest = e.get("sha256", "")
            if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
                problems.append(f"set {name}: {e.get('name')} has no SHA-256")
            if e.get("name") in seen:
                problems.append(f"set {name}: {e['name']} appears twice")
            seen.add(e.get("name"))
        if not s.get("sources"):
            problems.append(f"set {name}: no sources, so no provenance")
        for src, meta in (s.get("sources") or {}).items():
            if not meta.get("licence"):
                problems.append(f"set {name}: source {src} names no licence")
            if not meta.get("provenance"):
                problems.append(f"set {name}: source {src} names no provenance")
        if not s.get("materialise"):
            problems.append(f"set {name}: does not say how to materialise it")
    return problems


def self_test() -> int:
    """The checker must reject what it exists to reject."""
    good = build()
    failures = []

    if check(good):
        failures.append(f"a freshly built manifest was rejected: {check(good)}")

    import copy

    mutations = {
        "a dropped entry": lambda m: m["sets"]["gate"]["entries"].pop(),
        "an added entry": lambda m: m["sets"]["gate"]["entries"].append(
            {"name": "extra.pdf", "sha256": "0" * 64, "bytes": 1, "source": "x", "path": "x"}
        ),
        "a truncated digest": lambda m: m["sets"]["govdocs"]["entries"][0].update(
            {"sha256": "abc"}
        ),
        "a duplicated name": lambda m: m["sets"]["govdocs"]["entries"].__setitem__(
            1, dict(m["sets"]["govdocs"]["entries"][0])
        ),
        "a source without a licence": lambda m: m["sets"]["gate"]["sources"][
            "pdfjs"
        ].pop("licence"),
        "a set that cannot be materialised": lambda m: m["sets"]["govdocs"].pop(
            "materialise"
        ),
        "a missing set": lambda m: m["sets"].pop("govdocs"),
    }
    for label, mutate in mutations.items():
        broken = copy.deepcopy(good)
        mutate(broken)
        if not check(broken):
            failures.append(f"accepted {label}")

    for f in failures:
        print(f"FAIL {f}")
    if failures:
        return 1
    print(f"OK self-test: {len(mutations)} mutations rejected, the real manifest accepted")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", default=str(REPO / "corpus" / "extraction" / "tuning.json"))
    p.add_argument("--check", action="store_true", help="verify the committed manifest")
    p.add_argument("--self-test", action="store_true")
    args = p.parse_args()

    if args.self_test:
        return self_test()

    if args.check:
        path = pathlib.Path(args.out)
        if not path.exists():
            print(f"FAIL {path} does not exist")
            return 1
        problems = check(read_json(path))
        for problem in problems:
            print(f"FAIL {problem}")
        if problems:
            return 1
        fresh = build()
        if read_json(path) != fresh:
            print(f"FAIL {path} is not what the generator produces; rebuild it")
            return 1
        print(f"OK {path} is pinned and matches its generator")
        return 0

    manifest = build()
    problems = check(manifest)
    for problem in problems:
        print(f"FAIL {problem}")
    if problems:
        return 1
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1, sort_keys=True)
        f.write("\n")
    total = sum(len(s["entries"]) for s in manifest["sets"].values())
    print(f"OK wrote {out} -- {total} documents over {len(manifest['sets'])} sets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
