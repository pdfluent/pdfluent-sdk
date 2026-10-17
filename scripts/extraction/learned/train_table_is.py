"""Train the table-or-not model: is a table the builder made a table? (#638)

    train_table_is.py --dump TRAIN.jsonl [--cap 10] [--model-dir DIR]

T3 COUNTED THE HEADINGS WE MISS, and 41 % of them were a cell of a table
that is not one: a timeline in five columns, a list of officers with their
ages, labels beside their paragraphs. The judge decides among proposals and
the verifier among regions; neither asks whether the builder's table is a
table at all, and at the judge the heading line model has not run yet. This
model is read after it (`learned::tables`, over `CellHint`), on the whole
table: its shape, how many words its columns hold, how many figures, how
empty it is, and what the heading model said of its cells.

LABELS: a table is real when at least half of its cells lie in a DocLayNet
table region. DocLayNet TRAIN only, tables capped per document, rig refused,
and the strat-300 measure set refused by page name (#638).
THE THRESHOLD is chosen out of fold on training documents: the highest that
drops at most LOSS_BUDGET of the real tables.
Dumps from another engine tree are refused, unless --engine-skew-is-fine says
why, because the words of such a dump are not those of the engine that will
read the model; dumps whose verdicts another tree's decisions wrote are
refused, unless --decisions-skew-is-fine says why, because this model trains
ON the heading model's dumped p_heading_pass (#574) -- compared WITHOUT this
model's own directory, which its own output cannot have touched, so a retrain
reuses the dump instead of refusing it.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations
import argparse, collections, json, pathlib, random, statistics, sys, re
HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[2] / "ci"))
import align_labels as al  # noqa: E402
import pubtables_splits as splits  # noqa: E402
import training_rules  # noqa: E402

SEED = 20260930
LOSS_BUDGET = 0.01
THRESHOLDS = (0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.1, 0.15)
# A cell of running text holds this many words.
WORDY = 8
NUM = re.compile(r'^[\s$€£%()+\-–−.,0-9]+$')
NAMES=['rows','cols','cells','numeric','words_cell','col_words_max','col_words_min','heading_share','p_head_max','p_head_mean','first_col_p_head','empty','height_share','wordy_share','first_col_words']
# The strat-300 measure set, beside this script: 300 eval pages for measuring
# only, which no trainer reads. `main` refuses a truth file naming any of them
# by page name, before LightGBM loads (#638).
STRAT300 = HERE.parent / "pubtables_eval_strat300.txt"


def refuse_strat_pages(truth_path):
    """Refuse a training run whose PubTables truth reads the strat-300 set."""
    if not truth_path:
        return
    strat = splits.read_id_set(STRAT300)
    for page in json.loads(pathlib.Path(truth_path).read_text()):
        if page.get("name") in strat:
            raise SystemExit(
                f"a strat-300 page in the training truth: {page['name']}. "
                "That set is for measuring only; a model that has seen it "
                "cannot be measured on it.")
def tables_of(r):
    t=collections.defaultdict(list)
    for b in r['blocks']:
        if b['kind']=='table_cell' and b.get('table') is not None: t[b['table']].append(b)
    return t
def feats(r,cells):
    rows=1+max(c['row'] for c in cells); cols=1+max(c['column'] for c in cells)
    words=[c['n_words'] for c in cells]; ph=[c.get('p_heading_pass') or 0.0 for c in cells]
    bycol=collections.defaultdict(list)
    for c in cells: bycol[c['column']].append(c['n_words'])
    cm=[statistics.fmean(v) for v in bycol.values()]
    first=[c for c in cells if c['column']==0]
    y0=min(c['bbox'][1] for c in cells); y1=max(c['bbox'][3] for c in cells)
    return [rows,cols,len(cells),sum(1 for c in cells if NUM.match(c['text'] or ''))/len(cells),statistics.fmean(words),max(cm),min(cm),
            sum(1 for p in ph if p>=0.5)/len(ph),max(ph),statistics.fmean(ph),statistics.fmean([c.get('p_heading_pass') or 0 for c in first]) if first else 0.0,
            1-len(cells)/max(1,rows*cols),(y1-y0)/max(1.0,float(r['height'])),sum(1 for w in words if w>=8)/len(words),
            statistics.fmean([c['n_words'] for c in first]) if first else 0.0]
def label(r,cells):
    try: regions=al.regions_of(r)
    except (Exception,SystemExit): return None
    words=r.get('words') or []
    n=sum(1 for c in cells if al.claim(c,words,regions)[0]=='table')
    return int(n*2>=len(cells))


def label_pubtables(r, cells, truth):
    """PubTables carries table boxes, not regions: a table is real when at
    least half of its cells' centres lie in one of the page's truth tables."""
    height = float(r["height"])
    boxes = [[t["bbox"][0], height - t["bbox"][3], t["bbox"][2], height - t["bbox"][1]]
             for t in truth.get("blocks") or [] if t.get("kind") == "table"]
    def inside(c):
        x = (c["bbox"][0] + c["bbox"][2]) / 2
        y = (c["bbox"][1] + c["bbox"][3]) / 2
        return any(b[0] <= x <= b[2] and b[1] <= y <= b[3] for b in boxes)
    return int(2 * sum(1 for c in cells if inside(c)) >= len(cells))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dump", required=True)
    ap.add_argument("--cap", type=int, default=10)
    ap.add_argument("--pubtables", default=None, help="a PubTables-train dump (schema /10)")
    ap.add_argument("--pubtables-truth", default=None, help="its ground truth (pages with table boxes)")
    ap.add_argument("--trees", type=int, default=300)
    ap.add_argument("--model-dir", default=None)
    ap.add_argument("--save", default=None)
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--decisions-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump whose verdicts another tree's "
                         "decisions wrote, and say why; without it such a "
                         "dump is refused")
    args = ap.parse_args()
    # BEFORE LightGBM loads: a refused run refuses fast, and the refusal never
    # depends on the ML stack being installed.
    refuse_strat_pages(args.pubtables_truth)
    import lightgbm as lgb
    import numpy as np
    import learned_training_uses_no_rig_document as rig_guard
    rig = {e["document"] for e in rig_guard.rig_entries(HERE.parents[3]) if e.get("document")}
    per = collections.defaultdict(list)
    for line in open(args.dump):
        r = json.loads(line)
        if not str(r.get("source", "")).endswith("/train"):
            raise SystemExit(f"a training page is not from the train split: {r.get('source')}")
        # A FIGURE QUOTED FROM ANOTHER ENGINE'S WORDS is about that engine,
        # not this one -- and the p_heading_pass beside them is another
        # model's verdict, so a decision-only landing changes it without
        # changing the tree (#574). Compared WITHOUT this model's own
        # directory: a retrain changes the serving decisions, which would
        # refuse the dump the retrain itself needs, for values it cannot
        # have touched -- its own output reaches no refine-off record read
        # here (training_rules.SELF_EXCLUDABLE).
        training_rules.same_engine(r, args.dump, args.engine_skew_is_fine)
        training_rules.same_decisions(r, args.dump, args.decisions_skew_is_fine,
                                      exclude="table_is")
        if r["document"] in rig:
            continue
        for t, cells in tables_of(r).items():
            y = label(r, cells)
            if y is not None:
                per[r["document"]].append((feats(r, cells), y))
    if args.pubtables:
        truth = {p["name"]: p for p in json.loads(pathlib.Path(args.pubtables_truth).read_text())}
        for line in open(args.pubtables):
            r = json.loads(line)
            training_rules.same_engine(r, args.pubtables, args.engine_skew_is_fine)
            training_rules.same_decisions(r, args.pubtables, args.decisions_skew_is_fine,
                                          exclude="table_is")
            page = truth.get(r.get("page_id"))
            if not page or page["document"] in rig:
                continue
            for t, cells in tables_of(r).items():
                per[page["document"]].append((feats(r, cells), label_pubtables(r, cells, page)))
    rng = random.Random(SEED)
    x, y, docs = [], [], []
    for d in sorted(per):
        rows = per[d]
        rng.shuffle(rows)
        for f, lab in rows[:args.cap]:
            x.append(f); y.append(lab); docs.append(d)
    x = np.asarray(x, dtype=np.float32); y = np.asarray(y)
    order = sorted(set(docs)); rng.shuffle(order)
    fold = {d: i % 3 for i, d in enumerate(order)}
    f = np.asarray([fold[d] for d in docs])
    params = {"objective": "binary", "num_leaves": 15, "max_depth": 5, "learning_rate": 0.05,
              "min_data_in_leaf": 30, "seed": SEED, "deterministic": True, "force_row_wise": True,
              "num_threads": 4, "verbosity": -1}
    oof = np.zeros(len(y))
    for k in range(3):
        m = lgb.train(params, lgb.Dataset(x[f != k], label=y[f != k], feature_name=NAMES), args.trees)
        oof[f == k] = m.predict(x[f == k], num_threads=1)
    real, false = y == 1, y == 0
    print(f"{len(y)} tables from {len(order)} documents: {real.sum()} real, {false.sum()} false")
    allowed = []
    for t in THRESHOLDS:
        off = oof < t
        lost = (off & real).sum() / max(1, real.sum())
        print(f"  < {t:.2f}: drops {(off & false).sum()} false ({(off & false).sum() / max(1, false.sum()):.2f}), loses {(off & real).sum()} real ({lost:.4f})")
        if lost <= LOSS_BUDGET:
            allowed.append(t)
    chosen = max(allowed)
    print(f"chosen threshold {chosen:.2f}", flush=True)
    final = lgb.train(params, lgb.Dataset(x, label=y, feature_name=NAMES), args.trees)
    if args.save:
        final.save_model(args.save)
    if args.model_dir:
        import model_dir
        manifest = {
            "task": "table_is", "schema": "pdfluent.features/10", "seed": SEED,
            "corpus": "builder tables: doclaynet train at schema /10 (every third page) and pubtables-1m train",
            "documents": [{"document": d, "source": "pubtables-1m/train" if d.startswith("PMC") else "doclaynet-v1.2/train", "split": "train"} for d in order],
            "rows": {"train": int(len(x))}, "cap_per_document": args.cap,
            "threshold": chosen, "trees": int(final.num_trees()), "params": params,
            "rust": "crates/pdf-engine/src/learned/models/table_is",
        }
        skew = training_rules.engine_skew_manifest()
        if skew is not None:
            manifest["engine_skew"] = skew
        skew = training_rules.decisions_skew_manifest()
        if skew is not None:
            manifest["decisions_skew"] = skew
        model_dir.write(args.model_dir, final, x, NAMES, manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
