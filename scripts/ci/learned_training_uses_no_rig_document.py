#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""No learned model is trained on a document the rig measures (#598).

A model trained on a page it is later scored on has been scored on its own
answers, and every number the rig prints for it is then a statement about
memory. So every training manifest (`scripts/extraction/learned/models/*/
manifest.json`, written by train.py) is read against every document the
structured rig names -- `corpus/structured/*.json`, the tuning half AND the
sealed fifths alike, because tuning on the tuning half is what the rig is for
and training on it is not.

BY DOCUMENT, NOT BY PAGE. Page 3 and page 4 of one report share a template, a
font programme and a table style (benchmarks/HOLDOUTS.md). A training page from
a document the rig holds any page of is a leak, whether or not the page itself
is on the rig. So the comparison is on the document name as well as on the
page's hash and name.

AND FROM A DATASET THAT ALLOWS IT. A manifest's documents name their source;
a source that is a dataset must carry `training_use = "allowed..."` in
benchmarks/structured/comparison/DATASETS.toml. The repository's own fixtures
need no verdict.

AND NEVER THE COLMERGE EVAL (track A). T1's 50 column-merged tables are
track A's go/no-go eval, drawn from the TRAIN pool, so their documents
(PubTables, by document) and companies (FinTabNet, by company) must not
appear in any track-A training manifest:
`scripts/extraction/tables_a/colmerge_heldout.json` names them, and a
training record with a track-A source naming one fails. The check is
scoped to track-A sources -- other models are not scored on the
colmerge set, so their PubTables documents are unaffected.

AND NEVER THE HEADERBAND EVAL (track A). T4's header-band subset
companies are track A's second eval slice, drawn from the TRAIN pool
too, so they must not appear in any track-A training manifest either:
`scripts/extraction/tables_a/heldout-headerband.json` names them, and
a training record with a track-A source naming one fails the same
way. A record marked `train_use: "never"` fails in any training
manifest, with or without either reference beside it.

Exit codes:
  0  no training document is on the rig, and every dataset allows training
  1  one is, or a dataset does not allow it
  2  there is no rig manifest to compare with (not a pass)
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]

# Track-A training sources, as `build_manifests.py` spells them: the
# colmerge held-out refuses only records with these sources, so other
# models' PubTables documents never trip on track A's eval.
TRACK_A_PT_SOURCE = "pubtables-1m/pmc-open-access"
TRACK_A_FTN_SOURCE = "fintabnet/ibm-1.0.0"


def rig_entries(root: pathlib.Path) -> list[dict]:
    entries = []
    for path in sorted((root / "corpus" / "structured").glob("*.json")):
        data = json.loads(path.read_text(encoding="utf-8"))
        for key in ("pages", "documents", "entries", "items"):
            if isinstance(data.get(key), list):
                entries.extend(dict(e, _manifest=path.name) for e in data[key])
                break
        else:
            for value in data.values():
                if isinstance(value, list) and value and isinstance(value[0], dict) and "sha256" in value[0]:
                    entries.extend(dict(e, _manifest=path.name) for e in value)
    return entries


def training_verdicts(root: pathlib.Path) -> dict[str, str]:
    path = root / "benchmarks" / "structured" / "comparison" / "DATASETS.toml"
    if not path.exists():
        return {}
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    return {name: str(entry.get("training_use", "")) for name, entry in data.get("datasets", {}).items()}


def _heldout_file(root: pathlib.Path,
                  name: str) -> dict[str, set[str]] | None:
    """One held-out file as the check reads it, or None with no file.

    Beside the manifests in a draw, committed in the tree: the leakage
    check reads it wherever the root carries it, and skips the check
    where it does not -- a reference that is not there cannot refuse.
    """
    path = root / "scripts" / "extraction" / "tables_a" / name
    if not path.is_file():
        return None
    data = json.loads(path.read_text(encoding="utf-8"))
    return {"documents": set(data.get("documents") or []),
            "companies": set(data.get("companies") or [])}


def colmerge_heldout(root: pathlib.Path) -> dict[str, set[str]] | None:
    """Track A's colmerge held-out documents and companies."""
    return _heldout_file(root, "colmerge_heldout.json")


def headerband_heldout(root: pathlib.Path) -> dict[str, set[str]] | None:
    """Track A's header-band held-out documents and companies."""
    return _heldout_file(root, "heldout-headerband.json")


def problems(root: pathlib.Path) -> tuple[list[str], int, int]:
    rig = rig_entries(root)
    if not rig:
        print("SKIPPED (not a pass): corpus/structured/ names no rig document; there is "
              "nothing to compare training manifests with.", file=sys.stderr)
        raise SystemExit(2)
    by_sha = {e["sha256"]: e for e in rig if e.get("sha256")}
    by_name = {e["name"].removesuffix(".pdf"): e for e in rig if e.get("name")}
    by_document = {e["document"]: e for e in rig if e.get("document")}
    verdicts = training_verdicts(root)
    heldouts = (("colmerge", colmerge_heldout(root)),
                ("headerband", headerband_heldout(root)))

    found: list[str] = []
    manifests = sorted((root / "scripts" / "extraction" / "learned" / "models").glob("*/manifest.json"))
    documents = 0
    for manifest in manifests:
        data = json.loads(manifest.read_text(encoding="utf-8"))
        where = manifest.relative_to(root)
        # A MANIFEST THAT DOES NOT NAME ITS DOCUMENTS IS A FAILURE, not a
        # crash and not a pass. `table_region` and `table_structure` wrote
        # `documents` as a count per split -- {"train": 1446, "test": 482} --
        # and this loop walked the dict's KEYS, called `.get` on a string and
        # died with AttributeError. A guard that dies has checked nothing,
        # which is the one thing it must never look like (#600).
        listed = data.get("documents")
        if not isinstance(listed, list) or any(not isinstance(d, dict) for d in listed):
            found.append(f"{where}: `documents` is {type(listed).__name__}, not a list of "
                         "records. Write one {document, source, split} per training "
                         "document: a count cannot be compared with the rig.")
            continue
        for doc in listed:
            documents += 1
            # The local benchmark copy (owner decision 29-09-2026, #623):
            # whatever its pages hash to, a record that names that path is
            # refused.
            if any("pdfluent/pb-tuning" in str(v) for v in doc.values()):
                found.append(f"{where}: {doc.get('document') or doc.get('sha256')} comes from the "
                             "local benchmark copy pdfluent/pb-tuning, which is never trained on")
            hit = (by_sha.get(doc.get("sha256", "")) or by_name.get(str(doc.get("page_id", "")))
                   or by_document.get(doc.get("document", "")))
            if hit:
                found.append(f"{where}: {doc.get('document') or doc.get('sha256')} is on the rig "
                             f"({hit['_manifest']}, split {hit.get('split', '?')}, document "
                             f"{hit.get('document')})")
            if doc.get("train_use") == "never":
                found.append(f"{where}: {doc.get('document') or doc.get('sha256')} is marked "
                             "train_use never: an eval set never trains, whatever "
                             "manifest names it")
            for label, heldout in heldouts:
                if heldout is None:
                    continue
                source = str(doc.get("source", ""))
                if source == TRACK_A_PT_SOURCE \
                        and doc.get("document") in heldout["documents"]:
                    found.append(f"{where}: {doc.get('document')} is a {label} held-out "
                                 "document: track A's eval tables are drawn from it, so no "
                                 "track-A training manifest may name it")
                if source == TRACK_A_FTN_SOURCE \
                        and doc.get("company") in heldout["companies"]:
                    found.append(f"{where}: {doc.get('company')} is a {label} held-out "
                                 f"company ({doc.get('document')}): track A's eval tables "
                                 "are drawn from it, so no track-A training manifest may "
                                 "name it")
            source = str(doc.get("source", ""))
            if source and not source.startswith("fixtures"):
                # "doclaynet-v1.2/train" is the doclaynet row: the longest
                # dataset key the source's first segment starts with.
                head = source.split("/")[0].lower()
                keys = sorted((k for k in verdicts if head.startswith(k.lower())), key=len)
                verdict = verdicts.get(keys[-1]) if keys else None
                if verdict is None or not verdict.startswith("allowed"):
                    found.append(f"{where}: source {source!r} has no training_use = \"allowed...\" "
                                 "in benchmarks/structured/comparison/DATASETS.toml")
    return found, len(manifests), documents


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", type=pathlib.Path, default=REPO)
    args = ap.parse_args()
    found, manifests, documents = problems(args.root)
    if found:
        print(f"[learned-leakage] FAIL: {len(found)} problem(s):", file=sys.stderr)
        for line in found:
            print(f"  {line}", file=sys.stderr)
        print("  A model trained on a rig document is scored on its own answers. Drop the "
              "document from the training set and retrain.", file=sys.stderr)
        return 1
    tails = []
    for label, heldout in (("colmerge", colmerge_heldout(args.root)),
                           ("headerband", headerband_heldout(args.root))):
        if heldout is not None:
            tails.append(f"the {label} held-out enforced "
                         f"({len(heldout['documents'])} documents, "
                         f"{len(heldout['companies'])} companies)")
    tail = (", " + ", ".join(tails)) if tails else ""
    print(f"[learned-leakage] OK: {manifests} training manifest(s), {documents} document(s), "
          f"none on the rig, every dataset allows training{tail}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
