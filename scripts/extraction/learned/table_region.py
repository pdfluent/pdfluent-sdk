#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What a learned decision reads about a BLOCK that might be inside a table.

WHY THIS FILE EXISTS. The structure model of #600 reaches TEDS 0.7956 when it
is given the annotation's table box and 0.5975 when it is given the region our
own heuristics found. Measured on one table set with one split, the whole of
that difference is the REGION -- the engine's features and candidates
reproduce the Python pipeline to four decimal places, so there is nothing left
in the structure half to win. The region is the lever.

THE SIZE OF IT, on 12 DocLayNet train shards (11,568 pages, 8,620 table
regions):

    every block of the table we call a cell      1,360    15.8 %
    some blocks cells, some not                  5,088    59.0 %
    no block of it we call a cell                2,172    25.2 %

One table in six is cleanly ours. In BLOCK terms the same set is 80.6 per cent
table_cell, 17.9 paragraph, 1.3 heading, 0.2 list_item -- which is the number
T2 measured independently (#600) and the number this file has to move.

THE PARTIAL CLASS IS THE TARGET. It is four times the complete-miss class and
much cheaper to repair: the model only decides about blocks at the edge of
something we already found, rather than conjuring a table where we saw none.

TRAINED ON DOCLAYNET, EVALUATED ON PUBTABLES, which is T2's suggestion and
better than the document split I was using. DocLayNet labels EVERY block, so
"the annotation has no table here" means something; PubTables does not annotate
every table, and 55 of 72 apparent false positives there were real tables it
had simply not annotated (T2, #600). Training where the negatives are real and
measuring on a different corpus is also a stronger test than one corpus split
two ways.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import json
import math
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import features as ft
import model_dir

ROOT = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "doclaynet"
SEED = 20260925

# A RUN OF LEADER CHARACTERS. T2 measured this on DocLayNet train: a block
# carrying three or more of these in a row is a table 1,869 times of 2,001,
# 93.4 per cent, against 59 it calls text. The shape is a financial-statement
# row -- `FTE Adjustment........ $ 3,757 $ 3,676` -- and a table of contents
# is the same shape. Cheap here; in the engine the same fact would cost a
# change to the premise that a column divider is white, in three places.
LEADERS = re.compile(r"[.\-‐-―_·•]{3,}")
NUMERIC = re.compile(r"^[\s$€£(]*-?[\d,.]+[%)\s]*$")

EXTRA = [
    "leader_run",
    "numeric_share",
    "short_words",
    "kind_is_cell",
    "kind_is_paragraph",
    "prev_is_cell",
    "next_is_cell",
    "gap_above_line",
    "gap_below_line",
    "neighbours_within_a_line",
    "x_frac_of_widest",
]
NAMES = ft.BLOCK_FEATURES + EXTRA


def extra_vector(block: dict, page: dict, index: int, blocks: list) -> list[float]:
    """The features a REGION needs that a block on its own does not have.

    A block is inside a table mostly because of what is AROUND it: a row of
    figures under another row of figures, at the same left edge, one line
    apart. The per-block features of #598 describe the block; these describe
    its neighbourhood, and they are the ones a region decision turns on.
    """
    text = block.get("text") or ""
    words = text.split()
    numeric = sum(1 for w in words if NUMERIC.match(w))
    line = max(1e-6, float(block.get("size") or page["body"] or 10.0))

    def kind_of(k: int) -> str:
        return blocks[k]["kind"] if 0 <= k < len(blocks) else ""

    # How many blocks start within one line height of this one's top or
    # bottom -- a table row has neighbours above and below at a line's
    # distance, a paragraph in prose does not.
    top, bottom = block["bbox"][3], block["bbox"][1]
    close = 0
    for other in blocks:
        if other is block:
            continue
        if abs(other["bbox"][3] - top) <= line or abs(other["bbox"][1] - bottom) <= line:
            close += 1

    widest = max((b["bbox"][2] - b["bbox"][0]) for b in blocks) if blocks else 1.0
    mine = block["bbox"][2] - block["bbox"][0]
    return [
        1.0 if LEADERS.search(text) else 0.0,
        numeric / max(1, len(words)),
        sum(1 for w in words if len(w) <= 3) / max(1, len(words)),
        1.0 if block["kind"] == "table_cell" else 0.0,
        1.0 if block["kind"] == "paragraph" else 0.0,
        1.0 if kind_of(index - 1) == "table_cell" else 0.0,
        1.0 if kind_of(index + 1) == "table_cell" else 0.0,
        _num(block.get("gap_above_em")),
        _num(block.get("gap_below_em")),
        float(close),
        mine / max(1e-6, widest),
    ]


def _num(v) -> float:
    return math.nan if v is None else float(v)


def vector(block: dict, page: dict, index: int, blocks: list) -> list[float]:
    return ft.block_vector(block) + extra_vector(block, page, index, blocks)


def read_pages(shards: int, split: str = "train", stride: int = 1):
    """One record per page, blocks included, from the feature shards.

    STRIDE, BECAUSE THE SHARDS ARE SORTED BY GENRE. The first twelve train
    shards are financial reports and nothing else, where 61.7 per cent of
    blocks sit inside a table; over DocLayNet as a whole it is nowhere near
    that. Reading the first N shards therefore trains and MEASURES on one
    genre, and reports a number that will not survive contact with the other
    five. Taking every `stride`-th shard spans them, and `main` prints the mix
    so a skew is visible in the output rather than in nobody's head.
    """
    available = sorted((ROOT / "features").glob(f"{split}-*.jsonl.gz"))
    chosen = available[::stride][:shards] if stride > 1 else available[:shards]
    for p in chosen:
        with gzip.open(p, "rt") as f:
            for line in f:
                yield json.loads(line)


def read_labels(page_ids: set, split: str = "train") -> dict:
    """`(page_id, block index) -> (DocLayNet label, share, region)`.

    From T3's alignment rather than from the COCO boxes in the feature
    records: that join has been measured and this one would be a fourth
    coordinate frame to get wrong (#598, #600).
    """
    out = {}
    path = ROOT / "alignment" / f"{split}-rows.jsonl"
    with open(path) as f:
        for line in f:
            row = json.loads(line)
            if row.get("row") != "block" or row["page_id"] not in page_ids:
                continue
            out[(row["page_id"], row["i"])] = (row["label"], row["share"],
                                               row["region"])
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--shards", type=int, default=12)
    ap.add_argument("--stride", type=int, default=6,
                    help="take every Nth shard, because they are "
                         "sorted by genre and the first ones are "
                         "all financial reports")
    ap.add_argument("--trees", type=int, default=300)
    ap.add_argument("--threads", type=int, default=4,
                    help="LightGBM threads. Recorded in the manifest, because\n"
                         "`deterministic` makes a run repeatable for a GIVEN\n"
                         "count and says nothing about another one: the same\n"
                         "data at 1 thread and at 4 gives identical scores and\n"
                         "different bytes (T3, T4, #602)")
    ap.add_argument("--leaves", type=int, default=31)
    # THE SPEED BUDGET IS TREES x DEPTH, and depth is what runs away. LightGBM
    # grows leaf-wise, so 31 leaves reached depth 20 and 296 x 20 = 5,920
    # comparisons per block, over the 5,000 of
    # `scripts/ci/a_learned_model_stays_in_budget.py`. Capping the depth caps
    # the longest path directly; capping the leaves would not.
    ap.add_argument("--max-depth", type=int, default=12)
    ap.add_argument("--false-table-weight", type=float, default=1.0,
                    help="how much more a PICTURE, LIST_ITEM or FORMULA block "
                         "counts as a negative. Those three are 45.8 per cent "
                         "of what the model wrongly calls a table, and they "
                         "are the shapes that cost the text axis (T2, #600)")
    ap.add_argument("--share", type=float, default=0.5,
                    help="how much of a block the table region must cover "
                         "before the block counts as inside it")
    ap.add_argument("--save-model", default=None)
    ap.add_argument("--model-dir", default=None,
                    help="write model.json.gz, manifest.json and "
                         "parity.json for export_rust.py")
    ap.add_argument("--validation-share", type=float, default=0.25)
    args = ap.parse_args()

    import numpy as np
    import lightgbm as lgb

    pages = list(read_pages(args.shards, stride=args.stride))
    mix = collections.Counter(p.get("category") for p in pages)
    print(f"pages: {len(pages)}")
    for genre, n in mix.most_common():
        print(f"  {genre:<22}{n:>7}  {100*n/len(pages):5.1f}%")
    labels = read_labels({p["page_id"] for p in pages})
    print(f"aligned blocks: {len(labels)}", flush=True)

    # SPLIT BY DOCUMENT, not by page: two pages of one annual report share a
    # house style, and a model that has seen one has half-seen the other.
    documents = sorted({p["document"] for p in pages})
    import random
    random.Random(SEED).shuffle(documents)
    cut = int(len(documents) * (1.0 - args.validation_share))
    training = set(documents[:cut])
    print(f"documents {len(documents)}: {len(training)} train, "
          f"{len(documents) - len(training)} test", flush=True)

    x_train, y_train, w_train, x_test, y_test = [], [], [], [], []
    test_labels = []
    test_pages = []
    for page in pages:
        pid = page["page_id"]
        blocks = page["blocks"]
        info = {"body": page.get("body_size"), "width": page["width"],
                "height": page["height"]}
        rows, truth, names_here, weights_here = [], [], [], []
        # THE SHAPES THAT COST SOMETHING. A chart's data labels, a column of
        # list items and a displayed equation are what the model turns into
        # tables, and each of them is a whole false table rather than a ragged
        # edge. Weighting them as negatives is cheaper than a second model and
        # it is the thing being asked of the first one.
        for i, b in enumerate(blocks):
            label, share, _region = labels.get((pid, b["i"]), (None, 0.0, None))
            rows.append(vector(b, info, i, blocks))
            inside = 1 if label == "table" and share >= args.share else 0
            truth.append(inside)
            names_here.append(label)
            weights_here.append(
                args.false_table_weight
                if inside == 0 and label in ("picture", "list_item", "formula")
                else 1.0)
        if page["document"] in training:
            x_train.extend(rows)
            y_train.extend(truth)
            w_train.extend(weights_here)
        else:
            x_test.extend(rows)
            y_test.extend(truth)
            test_labels.extend(names_here)
            test_pages.append((pid, blocks, truth, len(x_test) - len(rows)))

    x = np.asarray(x_train, dtype=np.float32)
    y = np.asarray(y_train, dtype=np.int32)
    w = np.asarray(w_train, dtype=np.float32)
    print(f"training blocks: {len(y)} ({100*y.mean():.1f}% inside a table); "
          f"test blocks: {len(y_test)}", flush=True)

    # THE NATIVE API, NOT THE SKLEARN WRAPPER. `lgb.LGBMClassifier` needs
    # scikit-learn, and the training venv is three pinned packages on purpose
    # (`setup_venv.sh`): the crash it exists to prevent was two OpenMP runtimes
    # in one process, and scikit-learn brings a numeric stack of its own. So
    # every model in this directory goes through `lgb.train`, the way
    # `train_lists.py` always did (T6, #598).
    params = {
        "objective": "binary",
        "num_leaves": args.leaves,
        "max_depth": args.max_depth,
        "learning_rate": 0.1,
        "min_data_in_leaf": 100,
        "seed": SEED,
        "deterministic": True,
        "force_row_wise": True,
        "num_threads": args.threads,
        "verbosity": -1,
    }
    cut = int(len(x) * 0.85)
    booster = lgb.train(
        params,
        lgb.Dataset(x[:cut], label=y[:cut], weight=w[:cut], feature_name=NAMES),
        num_boost_round=args.trees,
        valid_sets=[lgb.Dataset(x[cut:], label=y[cut:], weight=w[cut:])],
        callbacks=[lgb.early_stopping(25, verbose=False)])
    trees = booster.best_iteration or args.trees
    print(f"  stopped at {trees} of {args.trees}", flush=True)
    if args.save_model:
        booster.save_model(args.save_model)
        print(f"  model written to {args.save_model}", flush=True)

    if args.model_dir:
        model_dir.write(args.model_dir, booster, x, NAMES, {
            "task": "table_region",
            "schema": "pdfluent.features/1",
            "seed": SEED,
            "corpus": "doclaynet/train",
            "shards": args.shards,
            "stride": args.stride,
            "genres": dict(mix),
            "pages": len(pages),
            # EVERY DOCUMENT BY NAME, not a count per split. The count is
            # what this wrote first, and `learned_training_uses_no_rig_document.py`
            # reads `documents` as a list of records: it iterated the dict's
            # keys, called `.get` on a string and died -- so the rig-leak guard
            # had never once checked this model (#600).
            "documents": [{"document": name, "source": "doclaynet-v1.2/train",
                           "split": "train" if name in training else "test"}
                          for name in documents],
            "rows": {"train": int(len(y)), "test": int(len(y_test))},
            "positives": int(y.sum()),
            "share": args.share,
            "trees": int(booster.num_trees()),
            "params": params,
            "threshold": 0.4,
            "rust": "crates/pdf-engine/src/learned/models/table_region",
        })
        print(f"  model directory written to {args.model_dir}", flush=True)

    said = booster.predict(np.asarray(x_test, dtype=np.float32),
                           num_iteration=trees, num_threads=1)
    truth = np.asarray(y_test, dtype=np.int32)

    print("\nPER BLOCK, at several thresholds:")
    print("  thr   precision  recall      F1   blocks called inside")
    best = None
    for thr in (0.3, 0.4, 0.5, 0.6, 0.7, 0.8):
        called = said >= thr
        tp = int((called & (truth == 1)).sum())
        fp = int((called & (truth == 0)).sum())
        fn = int((~called & (truth == 1)).sum())
        precision = tp / max(1, tp + fp)
        recall = tp / max(1, tp + fn)
        f1 = 2 * precision * recall / max(1e-9, precision + recall)
        print(f"  {thr:.1f}   {precision:8.3f}  {recall:6.3f}  {f1:6.3f}   {called.sum()}")
        if best is None or f1 > best[1]:
            best = (thr, f1)

    # WHAT THE FALSE POSITIVES ACTUALLY ARE. A precision number says how many
    # blocks the model wrongly calls table; it does not say whether they are
    # the ragged edge of a real table, which costs almost nothing, or whole
    # columns of list items, which destroys a list another lane fixed. T2
    # measured the same class from the text axis and found it carries 58 per
    # cent of our gap to Docling on 13 pages of 171 (#600).
    called = said >= 0.4
    wrong = called & (truth == 0)
    labels = collections.Counter()
    for i, bad in enumerate(wrong):
        if bad:
            labels[test_labels[i]] += 1
    total_wrong = sum(labels.values())
    print(f"\nWHAT THE MODEL WRONGLY CALLS A TABLE at 0.4 ({total_wrong} blocks):")
    for label, n in labels.most_common(8):
        print(f"  DocLayNet says {str(label):<18}{n:>7}  {100*n/max(1,total_wrong):5.1f}%")

    # THE BASELINE IS WHAT WE DO TODAY, and it has to be beaten, not merely
    # reported: `kind == table_cell` IS the region we hand the structure model.
    cell = np.asarray([row[NAMES.index("kind_is_cell")] for row in x_test]) == 1.0
    tp = int((cell & (truth == 1)).sum())
    fp = int((cell & (truth == 0)).sum())
    fn = int((~cell & (truth == 1)).sum())
    p0 = tp / max(1, tp + fp)
    r0 = tp / max(1, tp + fn)
    print(f"\n  what we ship today (kind == table_cell): "
          f"precision {p0:.3f}, recall {r0:.3f}, "
          f"F1 {2*p0*r0/max(1e-9,p0+r0):.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
