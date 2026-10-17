#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Train a learned decision offline, on this Mac (#598).

    python3 scripts/extraction/learned/train.py --task toy_heading \
        --features dump.jsonl [--features more.jsonl ...] \
        --out scripts/extraction/learned/models/toy_heading

Writes three files into --out:

  model.json     LightGBM's own dump of the trees (dump_model), the input of
                 export_rust.py.
  manifest.json  what the model was trained on: every document, the task, the
                 feature list, the seed, the library version. The leakage guard
                 (scripts/ci/learned_training_uses_no_rig_document.py) reads it.
  parity.json    feature vectors with LightGBM's own raw margins, the fixture
                 the Rust parity test compares the exported code against.

Deterministic: fixed seed, one thread, deterministic mode, so a retrain on the
same dump reproduces the checked-in model byte for byte.

TASKS

  toy_heading   "is this block a heading", labelled by OUR OWN classifier. It
                is a toy on purpose: it proves the dump, the training, the
                exporter and the parity test end to end on fixtures anyone can
                regenerate, with no dataset to fetch. A model that copies the
                heuristic is worthless as a replacement for it; the real tasks
                (#599, #600) label from DocLayNet and PubTables.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from features import BLOCK_FEATURES, READABLE, SCHEMA, block_vector  # noqa: E402

SEED = 598


def toy_heading_label(block: dict) -> int:
    return 1 if block.get("kind") == "heading" else 0


TASKS = {"toy_heading": toy_heading_label}


def read_records(paths: list[pathlib.Path]) -> list[dict]:
    records = []
    for path in paths:
        # split("\n"), not splitlines(): JSON may carry U+2028 raw (#598).
        for line in path.read_text(encoding="utf-8").split("\n"):
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("schema") not in READABLE:
                raise SystemExit(f"{path}: a record of schema {record.get('schema')!r}, "
                                 f"this trainer reads {sorted(READABLE)}")
            records.append(record)
    return records


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--task", required=True, choices=sorted(TASKS))
    ap.add_argument("--features", action="append", required=True, type=pathlib.Path)
    ap.add_argument("--source", default="fixtures",
                    help="where the documents come from, for the manifest")
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--rust", default=None,
                    help="where export_rust.py writes this model; recorded in the manifest")
    ap.add_argument("--trees", type=int, default=16)
    ap.add_argument("--leaves", type=int, default=7)
    args = ap.parse_args()

    import lightgbm as lgb  # imported here so --help works without it
    import numpy as np

    label_of = TASKS[args.task]
    records = read_records(args.features)
    rows, labels, documents = [], [], {}
    for record in records:
        documents[record["pdf_sha256"]] = {
            "source": args.source,
            "page_id": record["page_id"],
            "sha256": record["pdf_sha256"],
            "document": record.get("document") or record["pdf_sha256"],
        }
        for block in record["blocks"]:
            rows.append(block_vector(block))
            labels.append(label_of(block))
    if len(set(labels)) < 2:
        raise SystemExit("the training rows hold one class only; nothing to learn")

    x = np.asarray(rows, dtype=np.float64)
    y = np.asarray(labels, dtype=np.int32)
    params = {
        "objective": "binary",
        "num_leaves": args.leaves,
        "min_data_in_leaf": 1,
        "learning_rate": 0.3,
        "seed": SEED,
        "deterministic": True,
        "num_threads": 1,
        "verbosity": -1,
    }
    booster = lgb.train(params, lgb.Dataset(x, label=y, feature_name=BLOCK_FEATURES),
                        num_boost_round=args.trees)

    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "model.json").write_text(json.dumps(booster.dump_model(), indent=1, sort_keys=True) + "\n")
    # One thread here too: the OpenMP pool under predict() crashed this Mac
    # (anaconda under Rosetta), and a single thread is also what makes the
    # parity fixture reproducible.
    margins = booster.predict(x, raw_score=True, num_threads=1)
    (args.out / "parity.json").write_text(json.dumps({
        "features": BLOCK_FEATURES,
        "rows": [[None if np.isnan(v) else v for v in row] for row in x.tolist()],
        "margins": [float(m) for m in margins],
    }, indent=0) + "\n")
    (args.out / "manifest.json").write_text(json.dumps({
        "task": args.task,
        "rust": args.rust,
        "schema": SCHEMA,
        "features": BLOCK_FEATURES,
        "seed": SEED,
        "params": params,
        "trees": args.trees,
        "lightgbm": lgb.__version__,
        "rows": len(rows),
        "positives": int(y.sum()),
        "documents": sorted(documents.values(), key=lambda d: d["sha256"]),
    }, indent=1, sort_keys=True) + "\n")
    print(f"trained {args.task}: {len(rows)} blocks from {len(documents)} document(s), "
          f"{int(y.sum())} positive, {args.trees} trees -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
