#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A judge over table-region proposals, and the floor it decides at (#574).

    train_table_judge.py --train 'dln-train/*.jsonl' --validation dln-val.jsonl

WHAT IT CHOOSES BETWEEN. Two proposers see a page's tables differently: the
builder's regions (T2) and #632's alignment proposer (aligned column edges
over consecutive rows, broken by a chunk that crosses a gap). On DocLayNet
validation they score 0.313 and 0.455 alone; an oracle that picks, per page,
the proposals matching an annotated table reads 0.613. The judge is the model
of that choice. It proposes nothing of its own and never touches the grid.

THE UNIT IS A LINE, keyed by its index in the page's lines, so two proposals
are compared exactly and the label is exact too: a proposal is right when its
lines match one annotated `table` region's lines at IoU >= 0.5.

THE OWNER'S RULES, and where each is kept:
  * DocLayNet TRAIN only, rig documents refused, pages capped per document
    (`--cap`), so a 200-page annual report does not teach the judge its house
    style -- and dumps from another engine tree refused, unless
    `--engine-skew-is-fine` says why, because the blocks of such a dump are
    not those of the engine that will read the model (#574);
  * dumps whose verdicts another tree's decisions wrote refused, unless
    `--decisions-skew-is-fine` says why: the judge trains ON the list models'
    dumped `list_signal`, so a decision-only landing changes its inputs
    without changing the tree (#574);
  * the floor chosen OUT OF FOLD on the training documents, never on the
    validation pages the figure is quoted from;
  * the ceiling (the oracle choice) measured in the SAME run as the judge, on
    the same pages, before the judge's number is printed.
"""
from __future__ import annotations

import argparse
import collections
import glob
import json
import pathlib
import random
import re
import statistics
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[2] / "ci"))
import structured_axes as axes  # noqa: E402
import table_proposals as tp  # noqa: E402
import training_rules  # noqa: E402

SEED = 20260928
FOLDS = 3
F1_SLACK = 0.005
FLOORS = (0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5)
LEADER = re.compile(r"(\.\s?){4,}|(-\s?){4,}|(_\s?){4,}")

NAMES = [f"from_{k}" for k in tp.KINDS] + [
    "lines", "lines_share_of_page", "words_per_line", "width_share", "height_share",
    "numeric_share", "leader_share", "short_line_share",
    "table_cell_share", "paragraph_share", "list_share", "heading_share",
    "rows", "columns", "fill",
    "gap_above", "gap_below",
    "best_iou_regions", "best_iou_alignment", "regions_overlapped",
    "grid_fill", "grid_columns",
    # T2's refused two-row candidates (#2016) are offered too; whether the
    # detector itself kept one is part of what it says.
    "accepted",
    # A CHART'S LABELS ALIGN LIKE A TABLE (T2, a JWN scorecard): a row of
    # two-digit year ticks, and runs of figures with no text column.
    "year_tick_share", "alpha_line_share",
    # WHAT THE LIST MODELS SAY OF THE PROPOSAL'S LINES (T5, /7), so a list
    # set beside a table is known for one before the table pass.
    "list_mean", "list_max",
]


YEAR_TICK = re.compile(r"^['\u2019]?\d{2}$")
ALPHA = re.compile(r"[A-Za-z]{3}")


def features(page, lines, cand, source, regions_sets, align_sets, accepted=True):
    """One proposal's features, in NAMES order."""
    idx = sorted(cand)
    mine = [lines[i] for i in idx]
    heights = [l["bbox"][3] - l["bbox"][1] for l in lines] or [1.0]
    unit = statistics.median(heights) or 1.0
    x0 = min(l["bbox"][0] for l in mine); x1 = max(l["bbox"][2] for l in mine)
    y0 = min(l["bbox"][1] for l in mine); y1 = max(l["bbox"][3] for l in mine)
    words = sum(l["n_words"] for l in mine)
    kinds = collections.Counter(l["kind"] for l in mine)
    rows = tp.bands([l["bbox"] for l in mine], 1, 3)
    columns = tp.bands([l["bbox"] for l in mine], 0, 2)
    above = [y0 - l["bbox"][3] for j, l in enumerate(lines) if j not in cand and l["bbox"][1] >= y1 - 1e-6
             and l["bbox"][0] < x1 and l["bbox"][2] > x0]
    below = [l["bbox"][1] - y0 for j, l in enumerate(lines) if j not in cand and l["bbox"][3] <= y0 + 1e-6
             and l["bbox"][0] < x1 and l["bbox"][2] > x0]
    gap_above = (min(g for g in (l["bbox"][1] - y1 for j, l in enumerate(lines) if j not in cand
                                  and l["bbox"][1] >= y1 - 1e-6 and l["bbox"][0] < x1 and l["bbox"][2] > x0)
                     ) / unit) if above else 99.0
    gap_below = (min(y0 - l["bbox"][3] for j, l in enumerate(lines) if j not in cand
                     and l["bbox"][3] <= y0 + 1e-6 and l["bbox"][0] < x1 and l["bbox"][2] > x0) / unit
                 ) if below else 99.0
    iou_r = max((tp.iou(cand, s) for s in regions_sets if s != cand), default=0.0)
    iou_a = max((tp.iou(cand, s) for s in align_sets if s != cand), default=0.0)
    cells = [l for l in mine if l["kind"] == "table_cell"]
    return [1.0 if source == k else 0.0 for k in tp.KINDS] + [
        float(len(mine)),
        len(mine) / max(1, len(lines)),
        words / max(1, len(mine)),
        (x1 - x0) / max(1e-6, float(page["width"])),
        (y1 - y0) / max(1e-6, float(page["height"])),
        sum(l["digits"] for l in mine) / max(1, words),
        sum(1 for l in mine if LEADER.search(l["text"])) / len(mine),
        sum(1 for l in mine if l["n_words"] <= 3) / len(mine),
        kinds["table_cell"] / len(mine),
        kinds["paragraph"] / len(mine),
        kinds["list_item"] / len(mine),
        (kinds["heading"] + kinds["title"]) / len(mine),
        float(len(rows)),
        float(len(columns)),
        len(mine) / max(1, len(rows) * len(columns)),
        gap_above,
        gap_below,
        iou_r,
        iou_a,
        float(sum(1 for s in regions_sets if s & cand)),
        statistics.fmean(l["grid_fill"] for l in cells) if cells else float("nan"),
        statistics.fmean(l["grid_columns"] for l in cells) if cells else float("nan"),
        1.0 if accepted else 0.0,
        sum(1 for l in mine for t in l["text"].split() if YEAR_TICK.match(t)) / max(1, words),
        sum(1 for l in mine if ALPHA.search(l["text"])) / len(mine),
        _mean([l["list"] for l in mine]),
        _max([l["list"] for l in mine]),
    ]


def _mean(values):
    got = [v for v in values if v == v]
    return statistics.fmean(got) if got else float("nan")


def _max(values):
    got = [v for v in values if v == v]
    return max(got) if got else float("nan")


def page_rows(page):
    """Every proposal on one page with its features, its label and its text."""
    lines, regions = tp.page_lines(page)
    truth_sets = [t for t in (set(i for i, l in enumerate(lines) if l["region"] == ri)
                              for ri, reg in enumerate(regions) if reg["label"] == "table") if t]
    props = tp.proposals(lines, page)
    regions_sets = [c for c, s, _ in props if s not in ("alignment_lines", "split", "union")]
    align_sets = [c for c, s, _ in props if s == "alignment_lines"]
    out = []
    for cand, source, accepted in props:
        best = max((tp.iou(cand, t) for t in truth_sets), default=0.0)
        out.append({"x": features(page, lines, cand, source, regions_sets, align_sets, accepted),
                    "y": 1 if best >= 0.5 else 0, "iou": best, "lines": cand})
    return {"lines": lines, "truth": truth_sets, "props": out}


def choose(props, probs, floor):
    """Greedy by probability; a proposal sharing a line with a chosen one is dropped."""
    order = sorted(range(len(props)), key=lambda i: -probs[i])
    chosen, used = [], set()
    for i in order:
        if probs[i] < floor:
            break
        if props[i]["lines"] & used:
            continue
        chosen.append(props[i]["lines"]); used |= props[i]["lines"]
    return chosen


def f1(page, chosen):
    truth = [tp.text(page["lines"], t) for t in page["truth"]]
    s = axes.category_score(truth, [tp.text(page["lines"], c) for c in chosen])
    return s["f1"] if s.get("scored") else None


def load(pattern, cap, seed, engine_skew_is_fine=None,
         decisions_skew_is_fine=None):
    """Pages capped per document, rig refused, only pages with a proposal or a table."""
    import learned_training_uses_no_rig_document as rig_guard
    rig = {e["document"] for e in rig_guard.rig_entries(HERE.parents[3]) if e.get("document")}
    by_doc = collections.defaultdict(list)
    refused = 0
    for path in sorted(glob.glob(pattern)):
        for raw in open(path):
            r = json.loads(raw)
            # Train pages AND validation pages: a figure quoted from another
            # engine's blocks is about that engine, not this one (#574).
            training_rules.same_engine(r, path, engine_skew_is_fine)
            # AND FROM ANOTHER TREE'S DECISIONS: the judge trains on the dumped
            # `list_signal` beside the blocks, so a decision-only landing
            # changes its inputs without changing the tree (#574).
            training_rules.same_decisions(r, path, decisions_skew_is_fine)
            if r["document"] in rig:
                refused += 1
                continue
            by_doc[r["document"]].append(raw)
    rng = random.Random(seed)
    pages = []
    for doc in sorted(by_doc):
        raws = by_doc[doc]
        rng.shuffle(raws)
        for raw in raws[:cap]:
            page = json.loads(raw)
            try:
                rows = page_rows(page)
            except (Exception, SystemExit):
                continue
            if rows["props"] or rows["truth"]:
                rows["document"] = doc
                rows["source"] = page.get("source")
                pages.append(rows)
    print(f"[{pattern}] {len(pages)} pages from {len(by_doc)} documents (cap {cap}); "
          f"rig refused {refused}", flush=True)
    return pages


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--train", required=True, help="glob of DocLayNet TRAIN dumps")
    ap.add_argument("--validation", required=True, help="the DocLayNet validation dump")
    ap.add_argument("--cap", type=int, default=10, help="pages per document at most")
    ap.add_argument("--trees", type=int, default=200)
    ap.add_argument("--max-depth", type=int, default=6)
    ap.add_argument("--save", default=None)
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--decisions-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump whose verdicts another tree's "
                         "decisions wrote, and say why; without it such a "
                         "dump is refused")
    args = ap.parse_args()

    import lightgbm as lgb
    import numpy as np

    train = load(args.train, args.cap, SEED, args.engine_skew_is_fine,
                 args.decisions_skew_is_fine)
    wrong = {p["source"] for p in train if not str(p["source"]).endswith("/train")}
    if wrong:
        raise SystemExit(f"a training page is not from the train split: {sorted(wrong)}")
    valid = load(args.validation, args.cap, 632, args.engine_skew_is_fine,
                 args.decisions_skew_is_fine)

    docs = sorted({p["document"] for p in train})
    random.Random(SEED).shuffle(docs)
    fold_of = {d: i % FOLDS for i, d in enumerate(docs)}
    params = {"objective": "binary", "num_leaves": 31, "max_depth": args.max_depth,
              "learning_rate": 0.1, "min_data_in_leaf": 50, "seed": SEED,
              "deterministic": True, "force_row_wise": True, "num_threads": 4,
              "verbosity": -1}

    def fit(pages):
        x = np.asarray([r["x"] for p in pages for r in p["props"]], dtype=np.float32)
        y = np.asarray([r["y"] for p in pages for r in p["props"]], dtype=np.int32)
        return lgb.train(params, lgb.Dataset(x, label=y, feature_name=NAMES),
                         num_boost_round=args.trees)

    def probs(model, page):
        if not page["props"]:
            return []
        return list(model.predict(np.asarray([r["x"] for r in page["props"]], dtype=np.float32),
                                  num_threads=1))

    # OUT OF FOLD on the training documents: only these choose the floor.
    oof = {}
    for k in range(FOLDS):
        model = fit([p for p in train if fold_of[p["document"]] != k])
        for n, p in enumerate(train):
            if fold_of[p["document"]] == k:
                oof[n] = probs(model, p)
    # A TABLE ON A PAGE THAT HAS NONE costs the tables axis nothing -- it is
    # scored only where a table is annotated -- and costs the text axis a
    # paragraph read as cells. So the floor is the best F1 among the floors
    # that draw no more false tables on table-less pages than the regions do
    # today, and the count is printed beside the F1.
    def false_tables(pages, pick):
        blank = [p for p in pages if not p["truth"]]
        return sum(len(pick(p)) for p in blank) / max(1, len(blank))

    base_false = false_tables(train, lambda p: [r for r in p["props"] if r["x"][NAMES.index("from_builder")] == 1.0])
    sweep = {}
    for floor in FLOORS:
        got = [f1(p, choose(p["props"], oof[n], floor)) for n, p in enumerate(train)]
        got = [g for g in got if g is not None]
        index = {id(p): n for n, p in enumerate(train)}
        fp = false_tables(train, lambda p: choose(p["props"], oof[index[id(p)]], floor))
        sweep[floor] = (statistics.fmean(got), fp)
    print(f"\nout of fold, TRAINING pages (false tables per table-less page, regions today: {base_false:.3f}):")
    for floor, (v, fp) in sweep.items():
        print(f"  floor {floor:.2f}  F1 {v:.4f}  false tables {fp:.3f}")
    # THE HIGHEST FLOOR WITHIN F1_SLACK OF THE BEST. F1 falls slowly as the
    # floor rises and false tables fall fast (0.572 -> 0.208 per table-less
    # page for 0.003 of F1 on the first run), and "no worse than today" binds
    # nothing when today is 0.674. Both numbers are out of fold on training
    # documents; the validation pages choose nothing.
    best = max(v for v, _ in sweep.values())
    allowed = [f for f in FLOORS if sweep[f][0] >= best - F1_SLACK and sweep[f][1] <= base_false]
    chosen = max(allowed) if allowed else max(FLOORS, key=lambda f: sweep[f][0])
    print(f"chosen floor {chosen:.2f}", flush=True)

    final = fit(train)
    if args.save:
        final.save_model(args.save)

    # THE CEILING FIRST, in this run, on these pages.
    arms = collections.defaultdict(list)
    for p in valid:
        oracle = sorted(((r["iou"], r["lines"]) for r in p["props"] if r["y"]), key=lambda t: -t[0])
        picked, used = [], set()
        for _, c in oracle:
            if not c & used:
                picked.append(c); used |= c
        pr = probs(final, p)
        for name, sel in (("oracle choice (ceiling)", picked),
                          ("regions as they are", [r["lines"] for r in p["props"] if r["x"][NAMES.index("from_builder")] == 1.0]),
                          ("alignment alone", [r["lines"] for r in p["props"] if r["x"][NAMES.index("from_alignment_lines")] == 1.0]),
                          (f"judge at {chosen:.2f}", choose(p["props"], pr, chosen))):
            g = f1(p, sel)
            if g is not None:
                arms[name].append(g)
    blank = [p for p in valid if not p["truth"]]
    print(f"\nVALIDATION false tables per table-less page ({len(blank)} pages): regions "
          f"{sum(1 for p in blank for r in p['props'] if r['x'][0] == 1.0) / max(1, len(blank)):.3f}, "
          f"judge {sum(len(choose(p['props'], probs(final, p), chosen)) for p in blank) / max(1, len(blank)):.3f}")
    print(f"\nVALIDATION, {len(arms['oracle choice (ceiling)'])} pages with a table:")
    for name, v in arms.items():
        print(f"  {name:28s} {statistics.fmean(v):.4f}")
    if args.model_dir:
        import model_dir
        x = np.asarray([r["x"] for p in train for r in p["props"]], dtype=np.float32)
        manifest = {
            "task": "table_judge",
            "schema": "pdfluent.features/7",
            "seed": SEED,
            "corpus": "doclaynet train, re-dumped at schema /7",
            "documents": [{"document": d, "source": "doclaynet-v1.2/train", "split": "train"}
                          for d in sorted({p["document"] for p in train})],
            "rows": {"train": int(len(x))},
            "cap_per_document": args.cap,
            # CHOSEN OUT OF FOLD ON THE TRAINING DOCUMENTS: the highest floor
            # within F1_SLACK of the best, drawing no more false tables than
            # the builder does today. The sweep is in the log and the commit.
            "threshold": chosen,
            "trees": int(final.num_trees()),
            "params": params,
            "rust": "crates/pdf-engine/src/learned/models/table_judge",
        }
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        skew = training_rules.decisions_skew_manifest()
        if skew is not None:
            manifest["decisions_skew"] = skew
        model_dir.write(args.model_dir, final, x, NAMES, manifest)
        print(f"wrote {args.model_dir}", flush=True)
    gain = sorted(zip(final.feature_name(), final.feature_importance("gain")), key=lambda t: -t[1])
    total = sum(g for _, g in gain) or 1.0
    print("what it reads:", ", ".join(f"{n} {100 * g / total:.0f}%" for n, g in gain[:8]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
