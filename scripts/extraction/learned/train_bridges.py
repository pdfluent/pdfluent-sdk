#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Which words bridge two columns, and the floor at which the grid wants them (#574).

    train_bridges.py --dump dump5.jsonl --grids grids.jsonl.gz \\
        --rows sep-rows.txt --columns sep-cols.txt --model-dir out/table_column_bridge

WHAT IT IS FOR. A header drawn over two columns overlaps the words of both, so
the stripes join and every body cell under it lands in one column. On the
training half of PubTables that is 47 per cent of the spanning header cells.
Leaving the annotation's own spanning words out of the stripes lifts the grid
from TEDS 0.7999 to 0.8379; every merge rule tried on the finished grid bought
0.007 at most. This trains the model that names those words at serve time.

THE LABEL is the annotation's: a word is a bridge when the cell it sits in
spans more than one column. A word the annotation does not cover is dropped
from training, as the separation trainer drops an uncovered gap. The narrower
`--label crossing` (only words that overlap two annotated columns) was tried
and lost, 0.8049 against 0.8287 out of fold: leaving a whole spanning header
out of the stripes is what keeps its short words from joining them too.

WHAT IT READ, 27/28-09: out of fold on 3,538 training tables TEDS goes from
0.7999 without bridges to 0.8105 / 0.8259 / **0.8287** / 0.8286 / 0.8257 at
floors 0.01 / 0.02 / **0.03** / 0.05 / 0.08 -- a peak inside the sweep, so 0.03
ships. Low, because a word wrongly left out of the stripes still lands in the
column it overlaps, while a bridge left in joins two columns for every row.

THREE RULES FROM THE OWNER, and where each is kept:

  * TRAINING DOCUMENTS ONLY. The corpus manifest has to declare the training
    split (`model_dir.require_training_source`), and within it the 25 per cent
    of documents every other number in this lane is reported on is never
    trained on and never used to choose anything.
  * THE FLOOR IS CHOSEN ON TRAINING DOCUMENTS, out of fold. Three folds by
    document; each training table is decoded with the model that did NOT see
    its document, and the floor is the one whose TEDS is highest there. An
    in-sample sweep would choose for a model that has memorised its answers.
  * A CAP PER DOCUMENT. One PMC article can hold dozens of tables and thousands
    of words; without a cap the model learns that article's typesetting.
    `--cap` words per document, drawn with the lane's seed.

THE TEST HALF IS REPORTED BY `separations_teds.py --bridges`, which refuses to
print through a truth whose ceiling does not read 0.9085.

Dumps from another engine tree are refused, unless --engine-skew-is-fine says
why: the words of such a dump are not those of the engine that will read the
model (#574).
"""
from __future__ import annotations

import argparse
import gzip
import json
import pathlib
import random
import statistics
import sys
import tempfile
from multiprocessing import Pool

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import model_dir  # noqa: E402
import pubtables_alignment as pa  # noqa: E402
import pubtables_cells  # noqa: E402
import separation_features as sf  # noqa: E402
import structured_axes  # noqa: E402
import train_separations as ts  # noqa: E402
import training_rules  # noqa: E402

FLOORS = (0.01, 0.02, 0.03, 0.05, 0.08)
FOLDS = 3

_MODELS: dict = {}


def _init(rows, cols, folds):
    import lightgbm as lgb
    _MODELS["rows"] = lgb.Booster(model_file=rows)
    _MODELS["cols"] = lgb.Booster(model_file=cols)
    _MODELS["folds"] = [lgb.Booster(model_file=f) for f in folds]


def _score(job):
    """One training table, decoded without bridges and at every floor."""
    import numpy as np
    record, table, want, fold, floor = job
    out = {}
    for name, bridges, at in [("none", None, 0.5)] + [
            (f"{f:.2f}", _MODELS["folds"][fold], f) for f in FLOORS]:
        # UNDRESSED: the floor is a property of the stripes, and it was chosen
        # before the header, section and stub readings existed.
        html = ts.decode(record, table, _MODELS["rows"], _MODELS["cols"], floor, np,
                         bridges=bridges, bridge_floor=at, dressing=())
        out[name] = structured_axes.teds(want, html) if html else 0.0
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", type=pathlib.Path, required=True)
    ap.add_argument("--grids", type=pathlib.Path, required=True)
    ap.add_argument("--rows", required=True, help="the shipped row boundary model")
    ap.add_argument("--columns", required=True, help="the shipped column boundary model")
    ap.add_argument("--separation-floor", type=float, default=0.1,
                    help="`tables::SEPARATION_FLOOR`, which the boundary models ship with")
    ap.add_argument("--cap", type=int, default=1500,
                    help="words per document at most, drawn with the seed")
    ap.add_argument("--trees", type=int, default=200)
    ap.add_argument("--leaves", type=int, default=31)
    ap.add_argument("--max-depth", type=int, default=6)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--workers", type=int, default=3)
    ap.add_argument("--label", choices=("span", "crossing"), default="span",
                    help="`span`: the word's cell spans columns. `crossing`: "
                         "that, AND the word overlaps words of two different "
                         "annotated columns -- the words that actually join "
                         "two stripes")
    ap.add_argument("--save", default=None, help="the final model, LightGBM text")
    ap.add_argument("--folds-dir", default=None,
                    help="where the three out-of-fold models are kept; a "
                         "temporary directory when not given")
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    args = ap.parse_args()

    import lightgbm as lgb
    import numpy as np

    manifest = json.loads((ts.STORE / "pdf" / "MANIFEST.json").read_text())
    model_dir.require_training_source(manifest, str(ts.STORE / "pdf" / "MANIFEST.json"))
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    documents = sorted({p["document"] for p in manifest["pages"]})
    random.Random(ts.SEED).shuffle(documents)
    cut = int(len(documents) * 0.75)
    train_docs = documents[:cut]
    fold_of = {d: i % FOLDS for i, d in enumerate(train_docs)}
    grids = {}
    with gzip.open(args.grids, "rt") as handle:
        for line in handle:
            row = json.loads(line)
            if row["document"] in fold_of:
                grids[row["sha256"]] = row

    per_doc: dict[str, list] = {}
    tables = []
    for line in open(args.dump):
        record = json.loads(line)
        # The words are one tree's segmentation: a floor quoted from another
        # engine's words is about that engine, not this one (#574).
        training_rules.same_engine(record, str(args.dump), args.engine_skew_is_fine)
        page = by_sha.get(record["pdf_sha256"])
        if not page or page["document"] not in fold_of:
            continue
        height = float(record["height"])
        for table in (grids.get(record["pdf_sha256"], {}).get("tables") or []):
            if abs(height - float(table["page_height"])) > ts.HEIGHT_SLACK:
                continue
            cells = [{"row_nums": c["r"], "column_nums": c["c"], "pdf_bbox": c["b"],
                      "pdf_text_tight_bbox": c["t"], "pdf_text_content": c["x"],
                      "is_column_header": c["h"]} for c in table["cells"]]
            if len(cells) < 4:
                continue
            box = pa.page_frame(table["box"], height)
            words = [{"bbox": w["bbox"], "text": w["text"]} for w in (record.get("words") or [])
                     if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                     and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
            if len(words) < 4:
                continue
            want = pubtables_cells.html_of({"cells": cells})[0]
            if want:
                tables.append(({"height": height, "words": words, "rules": []},
                               {"pdf_table_bbox": table["box"]}, want,
                               fold_of[page["document"]], args.separation_floor))
            word_width = statistics.median([w["bbox"][2] - w["bbox"][0] for w in words]) or 1.0
            placed = pa.place([w["bbox"] for w in words], cells, height)
            column_of = {id(w): cells[s]["column_nums"][0] for w, s in zip(words, placed)
                         if s is not None and len(set(cells[s]["column_nums"])) == 1}
            for (w, x), spot in zip(sf.bridge_candidates(words, word_width), placed):
                if spot is None:
                    continue
                y = 1 if len(set(cells[spot]["column_nums"])) > 1 else 0
                if y and args.label == "crossing":
                    under = {column_of[id(o)] for o in words if id(o) in column_of
                             and w["bbox"][0] < o["bbox"][2] and w["bbox"][2] > o["bbox"][0]}
                    y = 1 if len(under) >= 2 else 0
                per_doc.setdefault(page["document"], []).append((x, y))

    capped = 0
    rows = []
    for doc in sorted(per_doc):
        mine = per_doc[doc]
        if len(mine) > args.cap:
            capped += 1
            mine = random.Random(f"{ts.SEED}:{doc}").sample(mine, args.cap)
        rows.extend((doc, x, y) for x, y in mine)
    y_all = np.asarray([y for _, _, y in rows], dtype=np.int32)
    print(f"training documents {len(per_doc)} ({capped} capped at {args.cap} words); "
          f"{len(rows)} words, {100 * y_all.mean():.1f}% bridges; {len(tables)} tables",
          flush=True)

    params = {
        "objective": "binary", "num_leaves": args.leaves, "max_depth": args.max_depth,
        "learning_rate": 0.1, "min_data_in_leaf": 100, "seed": ts.SEED,
        "deterministic": True, "force_row_wise": True,
        "num_threads": args.threads, "verbosity": -1,
    }

    def fit(picked):
        x = np.asarray([rows[i][1] for i in picked], dtype=np.float32)
        y = np.asarray([rows[i][2] for i in picked], dtype=np.int32)
        return lgb.train(params, lgb.Dataset(x, label=y, feature_name=sf.BRIDGE_NAMES),
                         num_boost_round=args.trees)

    # OUT OF FOLD: every training table is scored by a model that never saw
    # its document, and only those scores choose the floor.
    scratch = args.folds_dir or tempfile.mkdtemp(prefix="bridges-")
    pathlib.Path(scratch).mkdir(parents=True, exist_ok=True)
    fold_files = []
    oof = np.zeros(len(rows))
    for k in range(FOLDS):
        inside = [i for i, r in enumerate(rows) if fold_of[r[0]] != k]
        outside = [i for i, r in enumerate(rows) if fold_of[r[0]] == k]
        booster = fit(inside)
        path = f"{scratch}/fold{k}.txt"
        booster.save_model(path)
        fold_files.append(path)
        oof[outside] = booster.predict(
            np.asarray([rows[i][1] for i in outside], dtype=np.float32), num_threads=1)
        print(f"  fold {k}: {len(inside)} trained, {len(outside)} held", flush=True)

    print("\nout-of-fold, per word:")
    print(f"  {'floor':>6s}{'precision':>11s}{'recall':>9s}{'F1':>8s}")
    for f in FLOORS:
        call = oof >= f
        hit = int((call & (y_all == 1)).sum())
        p = hit / max(1, int(call.sum()))
        r = hit / max(1, int((y_all == 1).sum()))
        print(f"  {f:6.2f}{p:11.3f}{r:9.3f}{0 if p + r == 0 else 2 * p * r / (p + r):8.3f}")

    with Pool(args.workers, initializer=_init,
              initargs=(args.rows, args.columns, fold_files)) as pool:
        scored = list(pool.imap_unordered(_score, tables, chunksize=8))
    means = {k: statistics.fmean(s[k] for s in scored) for k in scored[0]}
    print(f"\nTEDS over {len(scored)} TRAINING tables, each decoded out of fold:")
    print(f"  no bridges    {means['none']:.4f}")
    for f in FLOORS:
        print(f"  floor {f:.2f}    {means[f'{f:.2f}']:.4f}")
    chosen = max(FLOORS, key=lambda f: (means[f"{f:.2f}"], -f))
    print(f"\nchosen floor: {chosen:.2f} ({means[f'{chosen:.2f}']:.4f} against "
          f"{means['none']:.4f} without)", flush=True)

    final = fit(list(range(len(rows))))
    if args.save:
        final.save_model(args.save)
    gain = sorted(zip(final.feature_name(), final.feature_importance("gain")),
                  key=lambda p: -p[1])
    total = sum(g for _, g in gain) or 1.0
    print("what it reads:", ", ".join(f"{n} {100 * g / total:.0f}%" for n, g in gain[:6]))
    if args.model_dir:
        x = np.asarray([r[1] for r in rows], dtype=np.float32)
        test_docs = documents[cut:]
        manifest = {
            "task": "table_column_bridge",
            "schema": "pdfluent.separations/1",
            "seed": ts.SEED,
            "corpus": "pubtables-1m over pmc open access",
            "documents": [{"document": d, "source": "pubtables-1m/structure",
                           "split": "train"} for d in sorted(train_docs)]
                         + [{"document": d, "source": "pubtables-1m/structure",
                             "split": "test"} for d in sorted(test_docs)],
            "rows": {"train": len(rows)},
            "cap_per_document": args.cap,
            # CHOSEN OUT OF FOLD ON THE TRAINING DOCUMENTS; the sweep is above
            # and in the commit that ships it.
            "threshold": chosen,
            "trees": int(final.num_trees()),
            "params": params,
            "rust": "crates/pdf-engine/src/learned/models/table_column_bridge",
        }
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        model_dir.write(args.model_dir, final, x, sf.BRIDGE_NAMES, manifest)
        print(f"wrote {args.model_dir}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
