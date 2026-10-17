#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The final-table trainer keeps its negatives (#638).

`scripts/extraction/learned/train_table_is_v2.py` used to draw ten rows from a
document regardless of class, so a document with many real tables kept almost
only positives and the few hosts it had were cut. This test holds the per-class
cap and what came with it:

- a document with 30 positives and 3 negatives keeps 10 and 3, each class
  shuffled with the seed; the pre-change flat cap keeps 10 total on the same
  input, so this test fails without the change;
- `--cap-negatives` defaults to `--cap`, and an explicit value is kept;
- the threshold grid runs 0.02 ... 0.60 in steps of 0.02 above 0.10, still
  chosen out of fold on the train documents with the 1 % loss budget;
- the OOF AUROC helper ranks correctly, ties averaged, NaN dropped, and None
  when one class is missing;
- after the cap each class is refused when five documents carry more than a
  fifth of it -- including a negatives-only concentration the split-level rule
  would pass -- and `--lopsided-is-fine` lets it through.

The lopsided override reports what it lets through (#638): per class after
the cap, the share of rows the top five documents hold, with those five
document names and row counts -- printed and in the manifest; `lopsided_reason`
in the manifest when `--lopsided-is-fine` is given; and a second out-of-fold
run without the document holding the most negatives, its OOF AUROC and
threshold table beside the main run's, both AUROCs in the manifest as
`oof_auroc` and `oof_auroc_without_<document>`. On a synthetic set where one
document holds most negatives, the report names it, the top-five share is
exact, and the without-it mask drops exactly its rows.

`--oof-out PATH` writes the TRAIN side after the main out-of-fold run: one
JSON line per training row after the cap -- document, page_id, bbox, label,
iou, fold, oof -- each row with its fold's probability, from the model that
never trained on it; without the flag nothing is written. On a synthetic set
the file holds one line per capped row with exactly those keys, each row
carrying its fold's probability; the cap passes each row's page id and box
through untouched, and the out-of-fold masks train on `f != k` and score
`f == k`, so the scored fold never trains the model that scores it.

And six mutations: the flat cap run on the same input must keep 10 total (so
the cap test is not green on the old behaviour); the concentration delegation
taken out of a copy of the trainer must let a concentrated set through (so the
guard is what refuses it, and not something else in the file); for the
report, the drop taken out must keep the excluded rows, the share delegation
taken out must zero the top-five share, and the threshold counting taken out
must miscount the refused split; and the OOF value taken out must write the
fold where the probability goes.
No lightgbm on the gate: everything here runs before any model is fitted.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import pathlib
import random
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
LEARNED = REPO / "scripts" / "extraction" / "learned"
TRAINER = LEARNED / "train_table_is_v2.py"

CAP_CALL = "cap_rows(per[d], args.cap, cap_neg, rng)"
NEG_CALL = "negative_cap(args.cap, args.cap_negatives)"
GUARD_CALL = "refuse_lopsided_classes(docs, y, allow=bool(args.lopsided_is_fine))"
GUARD_DELEGATION = "training_rules.refuse_a_lopsided_split(name, documents, mask, allow=allow)"
AUROC_MODEL = "auroc(y, oof)"
AUROC_SINGLE = 'NAMES.index("run_on_share")'
SWEEP = "for t in THRESHOLDS:"
FLAG = '"--cap-negatives"'
LOPSIDED_FLAG = '"--lopsided-is-fine"'
MANIFEST_NEG_CAP = '"cap_negatives_per_document"'
MANIFEST_POS = '"positives": int(real.sum())'
MANIFEST_NEG = '"negatives": int(false.sum())'
REPORT_CALL = "concentration_by_class(docs, y)"
EXCLUDED_CALL = "most_negative_document(docs, y)"
MASK_CALL = "keep_without(docs, excluded)"
ROW_MAIN_CALL = "threshold_row(y, oof, t)"
ROW_WITHOUT_CALL = "threshold_row(y[keep], oof_without[keep], t)"
MANIFEST_REASON = 'manifest["lopsided_reason"] = args.lopsided_is_fine'
MANIFEST_LOPSIDED = 'manifest["lopsided"]'
MANIFEST_OOF = '"oof_auroc": auc_model'
WITHOUT_KEY = '"oof_auroc_without_{excluded}"'
REPORT_PRINT = "after the cap the top "
BESIDE_PRINT = "| without "
DROP_BODY = "return [d != excluded for d in documents]"
SHARE_DELEGATION = "training_rules.share_of_the_top(documents, mask)"
COUNT_GUARD = "if s == s and s < threshold:"
OOF_FLAG = '"--oof-out"'
OOF_GUARD = "if args.oof_out:"
OOF_WRITE_CALL = "write_oof_out(args.oof_out, docs, page_ids, bboxes, y, ious, f, oof)"
OOF_ROW_META = 'r.get("page_id"), t["bbox"]'
OOF_TRAIN_MASK = "lgb.Dataset(x[f != k], label=y[f != k]"
OOF_SCORE_MASK = "oof[f == k] = m.predict(x[f == k]"
OOF_VALUE = '"oof": float(p)'


def load(source: str, name: str):
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False,
                                     dir=LEARNED) as f:
        f.write(source)
        path = pathlib.Path(f.name)
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        path.unlink()


def rows(n_pos: int, n_neg: int) -> list:
    """Synthetic (features, label, iou) rows with distinct IoUs."""
    return [([float(i)], 1, float(i)) for i in range(n_pos)] + \
        [([float(1000 + i)], 0, float(1000 + i)) for i in range(n_neg)]


class Recorder:
    """A stand-in RNG: records what it was asked to shuffle, then reverses."""

    def __init__(self):
        self.calls: list = []

    def shuffle(self, xs):
        self.calls.append(list(xs))
        xs.reverse()


def refuses(module, documents, labels, allow=False) -> bool:
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            module.refuse_lopsided_classes(documents, labels, allow=allow)
        return False
    except SystemExit:
        return True


def refuses_split(documents) -> bool:
    """The split-level rule, for the case the per-class guard must catch alone."""
    sys.path.insert(0, str(LEARNED))
    import training_rules
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            training_rules.refuse_a_lopsided_split(
                "split", documents, [True] * len(documents))
        return False
    except SystemExit:
        return True


def close(v, expected) -> bool:
    return v is not None and abs(v - expected) < 1e-12


def main() -> int:
    source = TRAINER.read_text(encoding="utf-8")
    real = load(source, "train_table_is_v2_cap_real")
    fails = []

    for call, name in ((CAP_CALL, "the per-class cap"),
                       (NEG_CALL, "the negatives default"),
                       (GUARD_CALL, "the per-class concentration guard"),
                       (AUROC_MODEL, "the model AUROC"),
                       (AUROC_SINGLE, "the run_on_share AUROC"),
                       (SWEEP, "the threshold sweep"),
                       (FLAG, "--cap-negatives"),
                       (LOPSIDED_FLAG, "--lopsided-is-fine"),
                       (MANIFEST_NEG_CAP, "the negatives cap in the manifest"),
                       (MANIFEST_POS, "the positives count in the manifest"),
                       (MANIFEST_NEG, "the negatives count in the manifest"),
                       (REPORT_CALL, "the per-class concentration report"),
                       (EXCLUDED_CALL, "the most-negative document"),
                       (MASK_CALL, "the without-it mask"),
                       (ROW_MAIN_CALL, "the main threshold counting"),
                       (ROW_WITHOUT_CALL, "the without-it threshold counting"),
                       (MANIFEST_REASON, "the lopsided reason in the manifest"),
                       (MANIFEST_LOPSIDED, "the concentration in the manifest"),
                       (MANIFEST_OOF, "the OOF AUROC in the manifest"),
                       (WITHOUT_KEY, "the without-it AUROC in the manifest"),
                       (REPORT_PRINT, "the printed concentration report"),
                       (BESIDE_PRINT, "the beside-the-main threshold table"),
                       (DROP_BODY, "the without-it drop"),
                       (SHARE_DELEGATION, "the share delegation"),
                       (COUNT_GUARD, "the threshold counting"),
                       (OOF_FLAG, "--oof-out"),
                       (OOF_GUARD, "the oof-out guard"),
                       (OOF_WRITE_CALL, "the OOF file write"),
                       (OOF_ROW_META, "the page id and box beside each row"),
                       (OOF_TRAIN_MASK, "the out-of-fold training mask"),
                       (OOF_SCORE_MASK, "the out-of-fold scoring mask"),
                       (OOF_VALUE, "the OOF probability in the file")):
        if call not in source:
            fails.append(f"{name} ({call!r}) is not in train_table_is_v2.py")
    if fails:
        for f in fails:
            print(f"FAIL: {f}", file=sys.stderr)
        return 1

    # THE DOCUMENT WITH 30 POSITIVES AND 3 NEGATIVES keeps 10 and 3.
    kept = real.cap_rows(rows(30, 3), 10, 10, random.Random(real.SEED))
    pos = [r for r in kept if r[1] == 1]
    neg = [r for r in kept if r[1] != 1]
    if len(pos) != 10 or len(neg) != 3:
        fails.append(f"30 positives and 3 negatives kept {len(pos)} and "
                     f"{len(neg)}; expected 10 and 3")
    if len({r[2] for r in kept}) != 13:
        fails.append("the kept rows are not 13 distinct rows of the input")
    # THE PRE-CHANGE CAP on the same input: ten rows shuffled and the first
    # ten kept. Ten total, almost only positives -- so the assertion above
    # fails without the change, and this test is testing the change.
    flat = rows(30, 3)
    random.Random(real.SEED).shuffle(flat)
    flat = flat[:10]
    flat_pos = sum(1 for r in flat if r[1] == 1)
    if len(flat) != 10 or len(kept) == len(flat):
        fails.append("the pre-change flat cap does not keep 10 total on this "
                     "input: the mutation cannot be made")
    if flat_pos <= len(flat) - flat_pos:
        fails.append("the pre-change flat cap kept as many negatives as "
                     "positives: the case no longer shows what the change fixes")

    # EACH CLASS SHUFFLED WITH THE SEED: both classes are offered to the RNG,
    # and the same seed keeps the same rows twice.
    rec = Recorder()
    rec_kept = real.cap_rows(rows(30, 30), 10, 5, rec)
    if len(rec.calls) != 2:
        fails.append(f"the cap shuffled {len(rec.calls)} class(es); expected 2")
    else:
        if sorted(r[2] for r in rec.calls[0]) != [float(i) for i in range(30)]:
            fails.append("the first shuffle was not over the 30 positives")
        if sorted(r[2] for r in rec.calls[1]) != [float(1000 + i) for i in range(30)]:
            fails.append("the second shuffle was not over the 30 negatives")
    if sum(1 for r in rec_kept if r[1] == 1) != 10 or \
            sum(1 for r in rec_kept if r[1] != 1) != 5:
        fails.append("30 positives and 30 negatives at cap 10/5 did not keep "
                     "10 and 5")
    again = real.cap_rows(rows(30, 3), 10, 10, random.Random(real.SEED))
    if sorted(r[2] for r in again) != sorted(r[2] for r in kept):
        fails.append("the same seed kept different rows twice")
    if real.cap_rows([], 10, 10, random.Random(real.SEED)) != []:
        fails.append("an empty document did not cap to empty")
    only_pos = real.cap_rows(rows(25, 0), 10, 10, random.Random(real.SEED))
    if len(only_pos) != 10:
        fails.append(f"25 positives alone kept {len(only_pos)}; expected 10")

    # --CAP-NEGATIVES DEFAULTS TO --CAP.
    if real.negative_cap(10, None) != 10 or real.negative_cap(7, None) != 7:
        fails.append("--cap-negatives did not default to --cap")
    if real.negative_cap(10, 4) != 4:
        fails.append("an explicit --cap-negatives was not kept")
    if real.make_parser().get_default("cap_negatives") is not None:
        fails.append("--cap-negatives defaults to a number, so --cap cannot "
                     "move it")

    # THE GRID runs 0.02 ... 0.60, in steps of 0.02 above 0.10, with the
    # 1 % loss budget on positives.
    grid = real.THRESHOLDS
    if grid[0] != 0.02 or grid[-1] != 0.60:
        fails.append(f"the grid runs {grid[0]} ... {grid[-1]}; expected "
                     "0.02 ... 0.60")
    if any(b <= a for a, b in zip(grid, grid[1:])):
        fails.append("the grid is not increasing")
    steps = {round(b - a, 9) for a, b in zip(grid, grid[1:]) if a >= 0.10}
    if steps != {0.02}:
        fails.append(f"the grid above 0.10 steps by {sorted(steps)}; "
                     "expected 0.02")
    if real.LOSS_BUDGET != 0.01:
        fails.append(f"the loss budget is {real.LOSS_BUDGET}; expected 0.01")

    # THE AUROC HELPER ranks correctly.
    if not close(real.auroc([0, 0, 1, 1], [0.1, 0.2, 0.8, 0.9]), 1.0):
        fails.append("a perfect ranking did not score 1.0")
    if not close(real.auroc([0, 0, 1, 1], [0.9, 0.8, 0.2, 0.1]), 0.0):
        fails.append("a reversed ranking did not score 0.0")
    if not close(real.auroc([0, 1, 0, 1], [0.5, 0.5, 0.5, 0.5]), 0.5):
        fails.append("a tied ranking did not score 0.5")
    if not close(real.auroc([0, 0, 1, 1], [0.1, 0.4, 0.35, 0.8]), 0.75):
        fails.append("the mixed ranking did not score 0.75")
    if real.auroc([1, 1], [0.2, 0.8]) is not None:
        fails.append("a single class did not score None")
    if not close(real.auroc([0, 1, 1], [0.1, 0.9, float("nan")]), 1.0):
        fails.append("a NaN score was not dropped with its label")

    # THE CONCENTRATION GUARD is per class, after the cap.
    concentrated_pos = (["big"] * 60 + [f"d{i:03d}" for i in range(40)],
                        [1] * 100)
    if not refuses(real, *concentrated_pos):
        fails.append("positives 60 % one document were not refused")
    # Ten of thirty negatives in one host: a third of the class, under five
    # per cent of the split -- refused per class, passed split-level.
    per_class_only = ([f"p{i:03d}" for i in range(200)] + ["bighost"] * 10 +
                      [f"n{i:03d}" for i in range(20)],
                      [1] * 200 + [0] * 30)
    if not refuses(real, *per_class_only):
        fails.append("negatives a third one host were not refused")
    if refuses_split(per_class_only[0]):
        fails.append("the negatives-only concentration is refused split-level "
                     "too: the case no longer shows the guard is per class")
    balanced = ([f"d{i:03d}" for i in range(100)] * 2, [1] * 100 + [0] * 100)
    if refuses(real, *balanced):
        fails.append("a balanced set was refused")
    if refuses(real, *concentrated_pos, allow=True):
        fails.append("--lopsided-is-fine did not let the lopsided set through")
    if GUARD_DELEGATION not in source:
        fails.append(f"the guard delegation ({GUARD_DELEGATION!r}) is not in "
                     "train_table_is_v2.py; the mutation cannot be made")
    else:
        mutant = load(source.replace(GUARD_DELEGATION, "None"),
                      "train_table_is_v2_cap_mutant")
        if refuses(mutant, *concentrated_pos):
            fails.append("with the guard delegation taken out, the lopsided "
                         "set is still refused -- something else refuses it, "
                         "and this test is not testing the guard")

    # THE REPORT names the document holding most negatives: bighost with 30
    # negatives and 5 positives, nine documents with 2 and 5 each -- 48
    # negatives and 50 positives, the top five carrying 38 and 25 of them.
    rep_docs, rep_labels = ["bighost"] * 35, [0] * 30 + [1] * 5
    for i in range(9):
        rep_docs += [f"h{i:02d}"] * 7
        rep_labels += [0] * 2 + [1] * 5
    if real.most_negative_document(rep_docs, rep_labels) != "bighost":
        fails.append("the report did not name bighost, the holder of most "
                     "negatives")
    report = real.concentration_by_class(rep_docs, rep_labels)
    neg, pos = report["negatives"], report["positives"]
    if neg["rows"] != 48 or pos["rows"] != 50:
        fails.append(f"the report counted {neg['rows']} negatives and "
                     f"{pos['rows']} positives; expected 48 and 50")
    if abs(neg["share"] - 38 / 48) > 1e-12:
        fails.append(f"the negatives top-five share is {neg['share']}; "
                     f"expected {38 / 48}")
    if len(neg["top"]) != 5 or neg["top"][0] != ("bighost", 30):
        fails.append(f"the negatives top five is {neg['top']}; expected "
                     "bighost with 30 first, five documents long")
    if abs(pos["share"] - 0.5) > 1e-12:
        fails.append(f"the positives top-five share is {pos['share']}; "
                     "expected 0.5")
    if dict(pos["top"]).get("bighost") != 5:
        fails.append(f"bighost's 5 positives are not in {pos['top']}")
    # THE WITHOUT-IT MASK drops exactly bighost's rows: 35 go, 63 stay, 18
    # negatives and 45 positives among them.
    mask = real.keep_without(rep_docs, "bighost")
    if sum(mask) != len(rep_docs) - 35:
        fails.append("the without-it mask did not drop exactly bighost's "
                     f"35 rows: {len(rep_docs) - sum(mask)} went")
    if any(d == "bighost" for d, k in zip(rep_docs, mask) if k):
        fails.append("a bighost row survived the without-it mask")
    kept_neg = sum(1 for lab, k in zip(rep_labels, mask) if k and lab != 1)
    kept_pos = sum(1 for lab, k in zip(rep_labels, mask) if k and lab == 1)
    if kept_neg != 18 or kept_pos != 45:
        fails.append(f"the without-it mask kept {kept_neg} negatives and "
                     f"{kept_pos} positives; expected 18 and 45")
    if real.most_negative_document(["b", "a"], [0, 0]) != "a":
        fails.append("a tie for most negatives did not break alphabetically")
    if real.most_negative_document(["a"], [1]) is not None:
        fails.append("a set without negatives still named a document to drop")
    if real.concentration_by_class([], []) != {
            "positives": {"share": 0.0, "top": [], "rows": 0},
            "negatives": {"share": 0.0, "top": [], "rows": 0}}:
        fails.append("an empty set did not report two empty classes")
    # THE THRESHOLD ROW counts both tables the same way: below 0.5 the two
    # false scores and one real score are refused, and NaN scores are kept.
    if real.threshold_row([0, 0, 1, 1], [0.1, 0.4, 0.35, 0.8], 0.5) != (2, 2, 1, 2):
        fails.append("the threshold row miscounted the refused split")
    if real.threshold_row([0, 1], [float("nan"), float("nan")], 0.5) != (0, 1, 0, 1):
        fails.append("NaN scores were not counted as kept")
    # THREE REPORT MUTATIONS: the drop taken out keeps the excluded rows, the
    # share delegation taken out zeroes the top-five share, and the threshold
    # counting taken out miscounts the refused split.
    mutant = load(source.replace(DROP_BODY, "return [True] * len(documents)"),
                  "train_table_is_v2_report_mutant_drop")
    if sum(mutant.keep_without(rep_docs, "bighost")) != len(rep_docs):
        fails.append("with the drop taken out, the without-it mask still "
                     "drops rows -- something else drops them, and this test "
                     "is not testing the drop")
    mutant = load(source.replace(SHARE_DELEGATION, "(0.0, [])"),
                  "train_table_is_v2_report_mutant_share")
    mutant_share = mutant.concentration_by_class(
        rep_docs, rep_labels)["negatives"]["share"]
    if abs(mutant_share - 38 / 48) < 1e-12:
        fails.append("with the share delegation taken out, the top-five "
                     "share is still exact -- something else counts it, and "
                     "this test is not testing the report")
    mutant = load(source.replace(COUNT_GUARD, "if False:"),
                  "train_table_is_v2_report_mutant_count")
    if mutant.threshold_row([0, 0, 1, 1], [0.1, 0.4, 0.35, 0.8], 0.5) == (2, 2, 1, 2):
        fails.append("with the threshold counting taken out, the refused "
                     "split is still counted -- something else counts it, "
                     "and this test is not testing the counting")

    # --OOF-OUT writes one line per capped row, each with its fold's
    # probability: six rows over three folds, fold k scoring 0.25/0.50/0.75.
    oof_documents = ["d0", "d0", "d1", "d1", "d2", "d2"]
    oof_pages = [f"p{i}" for i in range(6)]
    oof_boxes = [[float(i), 0.0, float(i) + 1.0, 1.0] for i in range(6)]
    oof_labels = [1, 0, 1, 0, 1, 0]
    oof_ious = [0.9, 0.1, 0.8, 0.2, 0.7, 0.3]
    oof_folds = [0, 1, 2, 0, 1, 2]
    fold_probability = {0: 0.25, 1: 0.5, 2: 0.75}
    oof_scores = [fold_probability[k] for k in oof_folds]

    def oof_lines(module):
        with tempfile.NamedTemporaryFile("w", suffix=".jsonl",
                                         delete=False) as f:
            path = pathlib.Path(f.name)
        try:
            module.write_oof_out(path, oof_documents, oof_pages, oof_boxes,
                                 oof_labels, oof_ious, oof_folds, oof_scores)
            return [line for line in
                    path.read_text(encoding="utf-8").split("\n")
                    if line.strip()]
        finally:
            path.unlink()

    lines = oof_lines(real)
    if len(lines) != 6:
        fails.append(f"--oof-out wrote {len(lines)} lines for 6 capped rows; "
                     "expected 6")
    else:
        for i, line in enumerate(lines):
            try:
                row = json.loads(line)
            except ValueError:
                fails.append(f"--oof-out line {i} is not JSON: {line[:60]}")
                break
            expected = {"document": oof_documents[i], "page_id": oof_pages[i],
                        "bbox": oof_boxes[i], "label": oof_labels[i],
                        "iou": oof_ious[i], "fold": oof_folds[i],
                        "oof": oof_scores[i]}
            if set(row) != set(expected):
                fails.append(f"--oof-out line {i} carries {sorted(row)}; "
                             f"expected {sorted(expected)}")
            elif row.get("fold") != oof_folds[i] or \
                    row.get("oof") != fold_probability[oof_folds[i]]:
                fails.append(f"--oof-out line {i} does not carry its fold's "
                             f"probability: fold {row.get('fold')}, oof "
                             f"{row.get('oof')}")
            elif row != expected:
                fails.append(f"--oof-out line {i} is {row}; expected "
                             f"{expected}")
    # THE CAP passes each row's page id and box through untouched: five
    # tuples in, five tuples out, the metadata of the row it kept.
    meta = [([float(i)], 1 if i < 4 else 0, float(i), f"p{i}",
             [0.0, 0.0, float(i), 1.0]) for i in range(8)]
    kept_meta = real.cap_rows(meta, 10, 10, random.Random(real.SEED))
    if len(kept_meta) != 8 or any(len(r) != 5 for r in kept_meta):
        fails.append("the cap dropped the page id or box a row carries")
    elif any(r[3] != f"p{int(r[2])}" or r[4] != [0.0, 0.0, r[2], 1.0]
             for r in kept_meta):
        fails.append("the cap mixed the page id or box between rows")
    # THE OOF MUTATION: the probability taken out writes the fold where the
    # probability goes, so the per-fold check above must fail on it.
    mutant = load(source.replace(OOF_VALUE, '"oof": float(fold)'),
                  "train_table_is_v2_oof_mutant_value")
    mutant_lines = oof_lines(mutant)
    if len(mutant_lines) == 6 and all(
            json.loads(line)["oof"] == fold_probability[oof_folds[i]]
            for i, line in enumerate(mutant_lines)):
        fails.append("with the OOF value taken out, each row still carries "
                     "its fold's probability -- something else writes it, "
                     "and this test is not testing the write")

    for f in fails:
        print(f"FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("table-is-v2 trainer: the cap is per class (10 and 3 kept of 30 and "
          "3), --cap-negatives defaults to --cap, the grid runs 0.02 ... 0.60 "
          "at the 1 % budget, the OOF AUROC is ranked with ties averaged, "
          "each class is refused past a fifth in five documents, and the "
          "override reports what it lets through (bighost named, 38/48 and "
          "25/50 top-five shares, its 35 rows dropped without it), and "
          "--oof-out writes one line per capped row with its fold's "
          "probability.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
