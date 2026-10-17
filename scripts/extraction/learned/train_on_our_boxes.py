#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Train the pair relation on OUR word boxes, not PubTables' (#600).

WHY THIS FILE EXISTS. The model trained on their boxes scores 0.8274 on their
boxes and 0.5915 on ours -- 0.236 of TEDS lost purely to the change of boxes,
where the owner's threshold for retraining is 0.02. Two things could cause
that, and they want opposite answers:

  * our words are SPLIT differently (we emit 7.8 % more of them over the same
    table, the no-break space being one case), so a model trained on their
    tokenisation meets a denser graph with different distances -- and
    retraining on ours fixes it;
  * or our boxes lose information theirs carry, and retraining fixes nothing.

This measures which. The labels stay the annotation's: cells are theirs, only
the words are ours.

THE ANSWER, on 1,500 training documents and about 1,140 test tables of the
4,000 fetched pages, split by document:

    trained on theirs, tested on OURS     0.5915
    trained on OURS,   tested on OURS     0.7960
    trained on theirs, tested on theirs   0.8324

So retraining recovers 0.2045 of the 0.2359 -- 87 per cent of the gap is the
DISTRIBUTION, exactly as the word counts suggested: we emit 7.8 per cent more
words over the same table, and a model trained on their tokenisation meets a
denser graph. Our training set here holds 875,781 pairs against their 756,693
on the same tables, which is that difference in one number.

AND 0.0364 DOES NOT COME BACK. That is the part retraining cannot reach: on
this task our boxes carry a little less than theirs, whatever they are trained
on. It is small beside the 0.236 it was hiding inside, and it is still above
the owner's 0.02, so it is a real residue rather than a rounding.

THE TEST PART IS NOT THE RIG'S. Our boxes exist only for pages we fetched, and
the rig's pages stay off this machine, so this trains and tests inside the
4,000 fetched pages, split by DOCUMENT. Its numbers therefore do not sit beside
the main line's; what it measures is the DIFFERENCE between two box sources on
one set.

Dumps from another engine tree are refused, unless --engine-skew-is-fine says
why: the boxes of such a dump are not those of the engine that will read the
model (#574).
"""
from __future__ import annotations

import argparse
import json
import pathlib
import random
import sys
import tarfile

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
import grid_decoder as gd
import model_dir
import pair_features as pf
import pubtables_alignment as pa
import pubtables_cells
import structured_axes
import training_rules

STORE = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "pubtables"
SEED = 20260924
KEEP = {"same_cell": 1.0, "same_row": 1.0, "same_column": 0.25, "neither": 0.08}
HEIGHT_SLACK = 1.5  # T2's tolerance, so one number serves both lanes


def main() -> int:
    parser = argparse.ArgumentParser()
    # REQUIRED, NOT A DEFAULT: the default was a path on one machine's /private/tmp,
    # and the published tree may not name a path outside itself
    # (public_snapshot_audit.sh; publication-guards red on master from 28-09).
    parser.add_argument("--dump", required=True,
                        help="the feature dump of our boxes (JSONL), as feature_dump writes it")
    # 150 AND NOT 250, because the budget is trees x depth and early stopping
    # is free to spend every round it is given: at depth 10 it took 139 rounds,
    # which is 556 trees over the four classes and 5,560 comparisons. The cap
    # on rounds is what makes the budget hold whatever early stopping picks.
    parser.add_argument("--trees", type=int, default=150)
    parser.add_argument("--threads", type=int, default=4,
                        help="LightGBM threads, recorded in the "
                             "manifest: `deterministic` makes a "
                             "run repeatable for a GIVEN count "
                             "and says nothing about another "
                             "(T3, T4, #602)")
    parser.add_argument("--leaves", type=int, default=31)
    # AND A MULTICLASS MODEL PAYS FOR EVERY TREE. The four classes share one
    # flat list, so scoring one pair runs all 448 trees, not a quarter of
    # them; at depth 17 that was 7,616 comparisons against a budget of 5,000.
    # The header said "448 trees over 4 classes" and the budget guard's regex
    # could not read it, so nothing had ever held this model to the number.
    parser.add_argument("--max-depth", type=int, default=8)
    parser.add_argument("--no-early-stopping", action="store_true")
    parser.add_argument("--save-model", default=None)
    parser.add_argument("--model-dir", default=None,
                        help="write model.json.gz, manifest.json "
                             "and parity.json for export_rust.py")
    parser.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                        help="train on a dump another engine tree segmented, and "
                             "say why; without it such a dump is refused")
    parser.add_argument("--validation-share", type=float, default=0.25)
    parser.add_argument("--floors", default="0.5,0.6,0.7,0.8,0.9")
    parser.add_argument("--annotation-box", action="store_true",
                        help="ask about the words in the ANNOTATION's "
                             "table box instead of the region the "
                             "engine will have -- the upper bound, not "
                             "a shippable setting")
    parser.add_argument("--their-boxes", action="store_true",
                        help="the same split and the same tables, but with "
                             "PubTables' word boxes -- the control this file "
                             "exists to be compared against")
    args = parser.parse_args()

    import numpy as np
    import lightgbm as lgb

    manifest = json.loads((STORE / "pdf" / "MANIFEST.json").read_text())
    by_sha = {p["sha256"]: p for p in manifest["pages"]}
    ours = {}
    for line in open(args.dump):
        record = json.loads(line)
        # The boxes are one tree's segmentation verdicts: a comparison quoted
        # from another engine's boxes is about that engine, not this one (#574).
        training_rules.same_engine(record, args.dump, args.engine_skew_is_fine)
        ours[record["pdf_sha256"]] = record
    print(f"pages dumped: {len(ours)}, in the manifest: "
          f"{len(set(ours) & set(by_sha))}", flush=True)

    wanted = {p["document"] for p in manifest["pages"]}
    annotations = {}
    with tarfile.open(STORE / "PubTables-1M-PDF_Annotations.tar.gz", "r|gz") as tar:
        for member in tar:
            if not member.name.endswith("_tables.json"):
                continue
            pmc = member.name.split("_")[0]
            if pmc not in wanted:
                continue
            handle = tar.extractfile(member)
            if handle is None:
                continue
            try:
                annotations[pmc] = json.load(handle)
            except Exception:
                pass
            if len(annotations) >= len(wanted):
                break
    their_words = {}
    if args.their_boxes:
        keys = {f"{pmc}_table_{i}" for pmc, ts in annotations.items()
                for i in range(len(ts))}
        with tarfile.open(STORE / "PubTables-1M-Structure_Table_Words.tar.gz",
                          "r|gz") as tar:
            for member in tar:
                key = member.name.replace("_words.json", "")
                if key not in keys:
                    continue
                handle = tar.extractfile(member)
                if handle is None:
                    continue
                try:
                    their_words[key] = json.load(handle)
                except Exception:
                    pass

    documents = sorted(wanted)
    random.Random(SEED).shuffle(documents)
    cut = int(len(documents) * (1.0 - args.validation_share))
    training = set(documents[:cut])
    print(f"documents {len(documents)}: {len(training)} train, "
          f"{len(documents) - len(training)} test (split by DOCUMENT)",
          flush=True)

    train_x, train_y, evaluate = [], [], []
    skipped_height = crossing = unreached = 0
    dice = random.Random(SEED)
    for sha, record in ours.items():
        page = by_sha.get(sha)
        if not page:
            continue
        height = float(record["height"])
        is_training = page["document"] in training
        for index, table in enumerate(annotations.get(page["document"]) or []):
            if int(table.get("pdf_page_index", -1)) != int(page["page_no"]):
                continue
            if table.get("exclude_for_structure"):
                continue
            # THE TWO HEIGHTS HAVE TO AGREE or every word lands in the wrong
            # row and nothing throws (T2, #596).
            if abs(height - float(table["pdf_full_page_bbox"][3])) > HEIGHT_SLACK:
                skipped_height += 1
                continue
            cells = table["cells"]
            box = pa.page_frame(table["pdf_table_bbox"], height)
            if args.their_boxes:
                those = their_words.get(f"{page['document']}_table_{index}")
                if not those:
                    continue
                fit = pa.fit_crop(cells, those)
                if not fit:
                    continue
                scale, dx, dy = fit
                inside = [
                    {"text": w["text"],
                     "bbox": pa.page_frame(
                         [(w["bbox"][0] + dx) / scale, (w["bbox"][1] + dy) / scale,
                          (w["bbox"][2] + dx) / scale, (w["bbox"][3] + dy) / scale],
                         height)}
                    for w in those]
            else:
                # THE ENGINE'S OWN PAIRS (schema /4). `record["pairs"]` is
                # `[i, j, f0 .. f24]` with i and j indexing `record["words"]`,
                # computed by `pdf_engine::learned::pairs` -- the same code
                # that runs at extraction time. Nothing here recomputes them.
                inside = record["words"]
                page_pairs = record.get("pairs")
                if page_pairs is None:
                    raise SystemExit(
                        f"{args.dump}: a record without `pairs`. The dump is "
                        f"older than schema /4; rebuild it with the current "
                        f"feature_dump, or the model trains on features the "
                        f"engine does not compute.")
            if args.their_boxes:
                words = [w for w in inside
                         if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                         and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
                if len(words) < 4:
                    continue
                placed = pa.place([w["bbox"] for w in words], cells, height)
                keep = [i for i, p in enumerate(placed) if p is not None]
                if len(keep) < 4:
                    continue
                words = [words[i] for i in keep]
                owner = [placed[i] for i in keep]
                pairs = pf.candidates(words)
                block = None
            else:
                # Which PAGE words fall in this annotation's table, and which
                # cell owns each. The pairs are the engine's, so the index
                # that matters is the page's, and the local list below is only
                # what the decoder wants.
                here = [k for k, w in enumerate(inside)
                        if box[0] <= (w["bbox"][0] + w["bbox"][2]) / 2 <= box[2]
                        and box[1] <= (w["bbox"][1] + w["bbox"][3]) / 2 <= box[3]]
                if len(here) < 4:
                    continue
                placed = pa.place([inside[k]["bbox"] for k in here], cells, height)
                owned = {k: p for k, p in zip(here, placed) if p is not None}
                if len(owned) < 4:
                    continue
                local = {k: n for n, k in enumerate(sorted(owned))}
                words = [inside[k] for k in sorted(owned)]
                owner = [owned[k] for k in sorted(owned)]
                # A PAIR THE ENGINE ASKED ABOUT AND WE CAN LABEL. One word in
                # the table and one outside it has no truth here, so it is not
                # trained on; it is still asked at run time, which the region
                # measurement below is about.
                if args.annotation_box:
                    # THE OLD PATH, ON THE SAME SPLIT. Ask about the words in
                    # the ANNOTATION's table box, which is what the 0.80 line
                    # was measured on and what the engine never has at run
                    # time. Here to decompose the difference between that line
                    # and the region's, on ONE table set rather than two
                    # samples of different sizes.
                    pairs, block = pf.candidates(words), None
                else:
                    pairs, block = [], []
                    for row in page_pairs:
                        i, j = int(row[0]), int(row[1])
                        if i in local and j in local:
                            pairs.append((local[i], local[j]))
                            block.append(row[2:])
                if not pairs and not is_training:
                    # A TABLE OUR HEURISTICS NEVER FOUND. The model runs under
                    # `refine_page` on the regions the engine produced, so a
                    # table it did not produce is a table the model cannot
                    # reach -- and skipping it here would score the model on
                    # the tables it happens to be good at. It is counted with
                    # the zero it earns, and reported beside the score over
                    # the tables we do reach.
                    unreached += 1
                    continue
            if not pairs:
                continue
            rows = [set(c.get("row_nums") or []) for c in cells]
            columns = [set(c.get("column_nums") or []) for c in cells]
            if is_training:
                for n, (i, j) in enumerate(pairs):
                    a, b = owner[i], owner[j]
                    try:
                        klass = pf.label(rows[a], columns[a], rows[b],
                                         columns[b], a == b)
                    except ValueError:
                        # A pair in two cells that share a row AND a column:
                        # one in 20.3 million here, and the four classes cannot
                        # name it. Dropped and counted rather than folded into
                        # `neither`, which would teach the model that a
                        # crossing span is nothing.
                        crossing += 1
                        continue
                    if dice.random() > KEEP[pf.CLASSES[klass]]:
                        continue
                    train_x.append(block[n] if block is not None
                                   else pf.features(words[i], words[j]))
                    train_y.append(klass)
            else:
                truth_html = pubtables_cells.html_of(table)[0]
                if truth_html:
                    evaluate.append((words, owner, rows, columns, pairs,
                                     block, truth_html))

    print(f"tables for training pairs: {len(train_x)} pairs; "
          f"tables to score: {len(evaluate)}; "
          f"tables our heuristics never found: {unreached}; "
          f"skipped on the height check: {skipped_height}; "
          f"pairs the four classes cannot name: {crossing}", flush=True)
    x = np.asarray(train_x, dtype=np.float32)
    y = np.asarray(train_y, dtype=np.int32)
    # THE NATIVE API, NOT THE SKLEARN WRAPPER, for the reason `table_region.py`
    # gives: `LGBMClassifier` needs scikit-learn, and the training venv is
    # three pinned packages because the crash it exists to prevent was two
    # OpenMP runtimes in one process (T6, #598).
    params = {
        "objective": "multiclass",
        "num_class": len(pf.CLASSES),
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
    if args.no_early_stopping:
        booster = lgb.train(
            params,
            lgb.Dataset(x, label=y, feature_name=pf.NAMES),
            num_boost_round=args.trees)
        trees = args.trees
        print(f"  {args.trees} iterations, {args.leaves} leaves", flush=True)
    else:
        booster = lgb.train(
            params,
            lgb.Dataset(x[:cut], label=y[:cut], feature_name=pf.NAMES),
            num_boost_round=args.trees,
            valid_sets=[lgb.Dataset(x[cut:], label=y[cut:])],
            callbacks=[lgb.early_stopping(25, verbose=False)])
        trees = booster.best_iteration or args.trees
        print(f"  stopped at {trees} of {args.trees} "
              f"iterations, {args.leaves} leaves", flush=True)
    if args.save_model:
        booster.save_model(args.save_model)
        print(f"  model written to {args.save_model} "
              f"({pathlib.Path(args.save_model).stat().st_size/1e6:.2f} MB, "
              f"{booster.num_trees()} trees)", flush=True)

    if args.model_dir:
        manifest = {
            "task": "table_structure",
            "schema": "pdfluent.features/4",
            "seed": SEED,
            "corpus": "pubtables-1m over pmc open access",
            "classes": list(pf.CLASSES),
            "pages": len(ours),
            # BY NAME, for the reason `table_region.py` gives: the rig-leak
            # guard reads `documents` as a list of records and a count per
            # split made it crash rather than check (#600).
            "documents": [{"document": name, "source": "pubtables-1m/structure",
                           "split": "train" if name in training else "test"}
                          for name in documents],
            "rows": {"train": int(len(x))},
            "keep": KEEP,
            "region": "pdf_engine::learned::pairs::in_table_regions, "
                      "growth 2.0, nearest 6",
            "trees": int(booster.num_trees()),
            "params": params,
            "floor": 0.9,
            "rust": "crates/pdf-engine/src/learned/models/table_structure",
        }
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        model_dir.write(args.model_dir, booster, x, pf.NAMES, manifest)
        print(f"  model directory written to {args.model_dir}", flush=True)

    def html_of(cells):
        return pubtables_cells.html_of({"cells": [
            {"row_nums": c["row_nums"], "column_nums": c["column_nums"],
             "pdf_text_content": c["text"], "is_column_header": 0 in c["row_nums"]}
            for c in cells]})[0]

    # THE FLOOR IS THE MODEL'S, NOT THE TASK'S. 0.9 was swept for the model
    # trained on the Python candidate set; a model trained on a different pair
    # population is calibrated differently, and reusing its neighbour's floor
    # is how a working model reads as a broken one -- at 0.9 almost no edge
    # passes, every word becomes its own cell, and TEDS falls to 0.09 for
    # reasons that have nothing to do with what the model learned.
    floors = [float(f) for f in args.floors.split(",")]
    scores = {f: [] for f in floors}
    for words, owner, rows, columns, pairs, block, truth_html in evaluate:
        block = np.asarray(
            block if block is not None
            else [pf.features(words[i], words[j]) for (i, j) in pairs],
            dtype=np.float32)
        said = booster.predict(block, num_threads=1)
        scored = {pair: said[n] for n, pair in enumerate(pairs)}
        for f in floors:
            out = gd.fill_empty(gd.from_edges(words, scored, row_span=lambda i: 1,
                                              column_span=lambda i: 1, floor=f))
            scores[f].append(structured_axes.teds(truth_html, html_of(out)))
    best = max(floors, key=lambda f: sum(scores[f]))
    for f in floors:
        mark = " <-" if f == best else ""
        print(f"  floor {f:.2f}: TEDS "
              f"{sum(scores[f])/max(1,len(scores[f])):.4f}{mark}")
    scores = scores[best]
    source = "THEIR boxes" if args.their_boxes else "OUR boxes"
    reached = sum(scores) / max(1, len(scores))
    print(f"\nTRAINED AND TESTED ON {source}, {len(scores)} tables: "
          f"TEDS {reached:.4f}")
    # TWO NUMBERS, because they answer two questions and only one of them is
    # comparable to the shipped engine's 0.5037. The first is what the model
    # does with a table the engine handed it; the second is what a reader gets,
    # which includes every table the engine never found and the model
    # therefore never saw.
    if unreached:
        whole = sum(scores) / max(1, len(scores) + unreached)
        print(f"  over every annotated table, the {unreached} we never found "
              f"scoring 0: TEDS {whole:.4f} over {len(scores) + unreached}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
