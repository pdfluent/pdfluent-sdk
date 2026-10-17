"""The table verifier: rank complete candidates, region and grid (#574, #638).

    table_verifier.py extract --dump D --set pubtables|doclaynet --out F [--truth T]
    table_verifier.py train --pubtables F --doclaynet F [--model-dir DIR]

THE LOSS IS THE REGION, NOT THE GRID. On 300 PubTables-train pages of
documents no training run reads, our decoder on the best proposal the page
already makes scores TEDS 0.852 and on the truth crop 0.872, while the
judge's chosen region scores 0.672: it hits the truth region (IoU >= 0.7) for
57 % of the tables. So the judge still decides WHETHER a candidate is a
table (its floor), and this model decides WHICH candidate: every candidate is
decoded, and the grid it makes is read for what a wrong crop leaves behind --
words cut at the edge, a table running on just past it, rows that stop being
regular, cells of running text.

LABELS. PubTables carries cells, so a candidate is graded by the TEDS of its
grid against the truth table; DocLayNet carries regions only, so by the
overlap of its lines with the truth region. Both on a 0..4 scale for
LambdaRank, pages capped per document, train splits only, rig refused, and
the strat-300 measure set refused by page name (#638).

`candidates` and `features` are the whole of what the Rust verifier
(`learned::verifier`) computes, and the parity test holds the two to it.
"""
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
from __future__ import annotations

import argparse
import collections
import gzip
import json
import math
import pathlib
import random
import statistics
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[2] / "ci"))

import pubtables_splits as splits  # noqa: E402
import train_table_judge as J  # noqa: E402

SEED = 20260930
# The candidates read per page: the judge's most likely, distinct by lines.
MOST_CANDIDATES = 12
# Row gaps under this share of a line are the same row.
ROW_GAP = 0.5
# A table running on is within this many lines of the hull.
NEAR = 1.5
# A caption sits within this many lines above the table.
CAPTION_REACH = 3.0
# A cell of running text: this many words, across all but one column.
PROSE_WORDS = 8
# Two candidates agree when their hulls overlap this much.
AGREE = 0.8
# A cell of a column of running text: this many words.
COLUMN_PROSE_WORDS = 6
# A column is read for running text when it holds this many cells.
COLUMN_CELLS = 3
MODELS = HERE.parent / "models"
# The strat-300 measure set, beside this script: 300 eval pages for measuring
# only, which no trainer reads. `train` refuses candidates naming any of them
# by page name, before LightGBM loads (#638).
STRAT300 = HERE.parent / "pubtables_eval_strat300.txt"

NAMES = [
    "judge_p", "from_builder", "words", "rows", "columns", "fill",
    "numeric_share", "header_gap", "row_cv", "column_residual", "near_miss",
    "edge_cut", "agreement", "caption_above", "prose_cells", "area_share",
    "spanning_share", "prose_column",
]

MISSING = {"None": 0, "Zero": 1, "NaN": 2}


class Trees:
    """A shipped model's own trees, predicted the way the Rust export does."""

    def __init__(self, name):
        directory = MODELS / name
        self.trees = json.loads(gzip.open(directory / "model.json.gz", "rt").read())["tree_info"]
        self.threshold = json.loads((directory / "manifest.json").read_text())["threshold"]

    def _leaf(self, node, x):
        while "leaf_value" not in node:
            v = float(x[node["split_feature"]])
            m = MISSING[node["missing_type"]]
            if m != 2 and v != v:
                v = 0.0
            if (m == 1 and abs(v) <= 1e-35) or (m == 2 and v != v):
                left = node["default_left"]
            else:
                left = v <= node["threshold"]
            node = node["left_child"] if left else node["right_child"]
        return node["leaf_value"]

    def margin(self, x):
        return sum(self._leaf(t["tree_structure"], x) for t in self.trees)

    def predict(self, xs, num_threads=1):
        import numpy as np
        return np.asarray([1.0 / (1.0 + math.exp(-self.margin(x))) for x in xs])


def median(values):
    return statistics.median(values) if values else 0.0


def centre(w):
    return ((w["bbox"][0] + w["bbox"][2]) / 2, (w["bbox"][1] + w["bbox"][3]) / 2)


def inside_of(words, hull):
    return [i for i, w in enumerate(words)
            if hull[0] <= centre(w)[0] <= hull[2] and hull[1] <= centre(w)[1] <= hull[3]]


def iou(a, b):
    ix = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - ix * iy
    return ix * iy / union if union > 0 else 0.0


def grid(words, models):
    """The decoder's cells for these words, as `separations::cells_of` makes
    them: (word indices, rows, columns) per cell, or None."""
    import numpy as np
    import train_separations as ts
    rows_model, cols_model, bridges = models
    if len(words) < 4:
        return None
    mine = [{"bbox": w["bbox"], "text": w["text"]} for w in words]
    text_height = median([w["bbox"][3] - w["bbox"][1] for w in mine]) or 1.0
    width = median([w["bbox"][2] - w["bbox"][0] for w in mine]) or 1.0
    bridge = ts.bridging(mine, width, bridges, bridges.threshold, np)
    row_of, col_of, spans = ts.grid_positions(mine, [], text_height, width, rows_model,
                                             cols_model, rows_model.threshold, np, bridge)
    slots = {(row_of[id(w)], col_of[id(w)]) for w in mine}
    boxes: dict = {}
    for w in mine:
        key = (row_of[id(w)], col_of[id(w)])
        if key in boxes:
            boxes[key] = (min(boxes[key][0], w["bbox"][0]),
                          max(boxes[key][1], w["bbox"][2]))
        else:
            boxes[key] = (w["bbox"][0], w["bbox"][2])
    _header, rowspans, spans = ts.dress(slots, spans, boxes=boxes)
    cells = collections.OrderedDict()
    for i, w in enumerate(mine):
        key = (row_of[id(w)], col_of[id(w)])
        cells.setdefault(key, []).append(i)
    # `cells_placed` refuses a grid of fewer than four cells.
    if len(cells) < 4:
        return None
    out = []
    for (r, c), held in cells.items():
        rows = rowspans.get((r, c), [r])
        columns = spans.get((r, c), [c])
        out.append((held, list(rows), list(columns)))
    return out


def candidates(props, probs):
    """The judge's most likely proposals, distinct by lines: indices."""
    seen, out = set(), []
    for i in sorted(range(len(probs)), key=lambda i: (-probs[i], i)):
        key = tuple(sorted(props[i]["lines"]))
        if not key or key in seen:
            continue
        seen.add(key)
        out.append(i)
        if len(out) >= MOST_CANDIDATES:
            break
    return out


def hull_of(lines, members):
    return [min(lines[j]["bbox"][0] for j in members), min(lines[j]["bbox"][1] for j in members),
            max(lines[j]["bbox"][2] for j in members), max(lines[j]["bbox"][3] for j in members)]


def features(size, lines, words, hull, hulls, me, p, builder, cells):
    """One candidate's features in NAMES order: `verifier::features`."""
    width, height = size
    unit = median([w["bbox"][3] - w["bbox"][1] for w in words]) or 1.0
    inside = inside_of(words, hull)
    held = set(inside)
    near = sum(1 for i, w in enumerate(words) if i not in held
               and hull[0] <= centre(w)[0] <= hull[2]
               and (0 < hull[1] - centre(w)[1] <= NEAR * unit or 0 < centre(w)[1] - hull[3] <= NEAR * unit))
    cut = sum(1 for i, w in enumerate(words) if i not in held
              and w["bbox"][0] < hull[2] and w["bbox"][2] > hull[0]
              and w["bbox"][1] < hull[3] and w["bbox"][3] > hull[1])
    ys = sorted(centre(words[i])[1] for i in inside)
    gaps = [b - a for a, b in zip(ys, ys[1:]) if b - a > ROW_GAP * unit]
    row_cv = statistics.pstdev(gaps) / statistics.fmean(gaps) if len(gaps) > 1 else 0.0
    lefts = [words[i]["bbox"][0] for i in inside]
    if len(lefts) > 2:
        residual = statistics.fmean(
            min(abs(a - b) for k, b in enumerate(lefts) if k != j) for j, a in enumerate(lefts)) / unit
    else:
        residual = 0.0
    agree = sum(1 for k, h in enumerate(hulls) if k != me and iou(h, hull) >= AGREE)
    caption = any(l["bbox"][1] >= hull[3] and l["bbox"][1] - hull[3] <= CAPTION_REACH * unit
                  and l["bbox"][0] < hull[2] and l["bbox"][2] > hull[0]
                  and table_caption(l["text"]) for l in lines)
    cells = cells or []
    n_rows = 1 + max((max(r) for _, r, _ in cells), default=-1)
    n_cols = 1 + max((max(c) for _, _, c in cells), default=-1)
    grid_words = [words[i] for i in inside]
    numeric = [any(ch in DIGITS for i in held_ for ch in grid_words[i]["text"])
               for held_, _, _ in cells]
    head = [n for (h, r, c), n in zip(cells, numeric) if min(r) == 0]
    body = [n for (h, r, c), n in zip(cells, numeric) if min(r) > 0]
    share = lambda xs: sum(xs) / max(1, len(xs))
    prose = sum(1 for h, r, c in cells if len(h) >= PROSE_WORDS and len(c) >= max(1, n_cols - 1))
    return [
        p, 1.0 if builder else 0.0, float(len(inside)), float(n_rows), float(n_cols),
        len(cells) / max(1, n_rows * n_cols), share(numeric), share(body) - share(head),
        row_cv, residual, float(near), float(cut), float(agree), 1.0 if caption else 0.0,
        float(prose), (hull[2] - hull[0]) * (hull[3] - hull[1]) / max(1.0, width * height),
        sum(1 for h, r, c in cells if len(c) > 1) / max(1, len(cells)),
        prose_column(cells),
    ]


def prose_column(cells):
    """The largest share of one column's own cells that hold running text:
    the other column of a two-column page, taken in as a column (#638)."""
    best = 0.0
    columns = collections.defaultdict(list)
    for h, r, c in cells:
        if len(c) == 1:
            columns[c[0]].append(len(h))
    for sizes in columns.values():
        if len(sizes) >= COLUMN_CELLS:
            best = max(best, sum(1 for n in sizes if n >= COLUMN_PROSE_WORDS) / len(sizes))
    return best


DIGITS = "0123456789"
ROMAN = {"I": 1, "V": 5, "X": 10, "L": 50, "C": 100, "D": 500, "M": 1000}


def table_caption(text):
    """`Table 2`, `TABLE IV:`, `Table S1` -- `tables::announces_a_table`, tables only."""
    words = text.split()
    if len(words) < 2 or words[0].rstrip(":").lower() != "table":
        return False
    ident = words[1].rstrip(".:,")
    if not ident:
        return False
    if ident[0] in DIGITS:
        return True
    if ident[0] in "Ss" and len(ident) > 1 and all(c in DIGITS for c in ident[1:]):
        return True
    if all("A" <= c <= "Z" for c in ident):
        values = [ROMAN[c] for c in ident]
        total = sum(-v if max(values[i + 1:], default=0) > v else v for i, v in enumerate(values))
        return 1 <= total <= 50
    return False


def without_title_rows(lines, words, members, inside, cells):
    """A candidate's lines without the title rows its grid opens with:
    `verifier::without_title_rows`."""
    cells = cells or []
    rows = 1 + max((max(r) for _, r, _ in cells), default=-1)
    low, high = math.inf, -math.inf
    for r in range(rows):
        here = [c for c in cells if min(c[1]) == r]
        if len(here) != 1 or len(here[0][2]) != 1 or r + 1 >= rows:
            break
        for w in here[0][0]:
            b = words[inside[w]]["bbox"]
            low, high = min(low, b[1]), max(high, b[3])
    if low > high:
        return list(members)
    kept = [j for j in members
            if not (low <= (lines[j]["bbox"][1] + lines[j]["bbox"][3]) / 2 <= high)]
    return kept or list(members)


def page_candidates(record, judge, models):
    """Every candidate on one dumped page: (proposal index, lines, x, grid words, cells)."""
    rows = J.page_rows(record)
    lines, props = rows["lines"], rows["props"]
    probs = [1.0 / (1.0 + math.exp(-judge.margin(r["x"]))) for r in props]
    size = (float(record["width"]), float(record["height"]))
    words = [{"bbox": w["bbox"], "text": w["text"]} for w in record.get("words") or []]
    picked = candidates(props, probs)
    hulls = [hull_of(lines, props[i]["lines"]) for i in picked]
    out = []
    nb = J.NAMES.index("from_builder")
    for k, i in enumerate(picked):
        inside = inside_of(words, hulls[k])
        cells = grid([words[j] for j in inside], models)
        x = features(size, lines, words, hulls[k], hulls, k, probs[i], props[i]["x"][nb] == 1.0, cells)
        out.append({"i": i, "lines": sorted(props[i]["lines"]), "hull": hulls[k], "x": x,
                    "p": probs[i], "cells": cells, "inside": inside})
    return rows, out


def html_of(words, inside, cells):
    import pubtables_cells
    grid_words = [words[i] for i in inside]
    return pubtables_cells.html_of({"cells": [
        {"row_nums": r, "column_nums": c,
         "pdf_text_content": " ".join(grid_words[j]["text"] for j in h), "is_column_header": 0}
        for h, r, c in cells]})[0] if cells else None


def _extract(args):
    raw, which, truth = args
    import structured_axes
    record = json.loads(raw)
    judge = Trees("table_judge")
    models = (Trees("table_row_boundary"), Trees("table_column_boundary"), Trees("table_column_bridge"))
    if which == "pubtables":
        page = truth.get(record.get("page_id"))
        if not page:
            return []
        record["regions"] = []
        record.pop("labels", None)
    try:
        rows, cands = page_candidates(record, judge, models)
    except (Exception, SystemExit):
        return []
    words = [{"bbox": w["bbox"], "text": w["text"]} for w in record.get("words") or []]
    out = []
    for c in cands:
        if which == "pubtables":
            height = float(record["height"])
            grades = []
            html = html_of(words, c["inside"], c["cells"])
            for t in page["blocks"]:
                box = [t["bbox"][0], height - t["bbox"][3], t["bbox"][2], height - t["bbox"][1]]
                grades.append(structured_axes.teds(t["html"], html)
                              if html and t.get("html") and iou(c["hull"], box) > 0 else 0.0)
            document = page["document"]
        else:
            mine = set(c["lines"])
            grades = [len(mine & t) / len(mine | t) for t in rows["truth"]]
            document = record["document"]
        out.append({"page": record.get("page_id"), "document": document, "x": c["x"],
                    "lines": c["lines"], "grade": max(grades, default=0.0)})
    return out


def extract(args):
    from concurrent.futures import ProcessPoolExecutor
    truth = {}
    if args.set == "pubtables":
        truth = {p["name"]: p for p in json.loads(pathlib.Path(args.truth).read_text())}
    with ProcessPoolExecutor(args.jobs) as ex, open(args.out, "w") as out:
        jobs = ((raw, args.set, truth) for raw in open(args.dump))
        for rows in ex.map(_extract, jobs, chunksize=4):
            for row in rows:
                out.write(json.dumps(row) + "\n")
    return 0


def refuse_strat_pages(paths):
    """Refuse a training run whose candidates read the strat-300 set (#638)."""
    strat = splits.read_id_set(STRAT300)
    for path in paths:
        for raw in open(path):
            name = json.loads(raw).get("page")
            if name in strat:
                raise SystemExit(
                    f"a strat-300 page in the training candidates: {name}. "
                    "That set is for measuring only; a model that has seen "
                    "it cannot be measured on it.")


def train(args):
    # BEFORE LightGBM loads: a refused run refuses fast, and the refusal never
    # depends on the ML stack being installed.
    refuse_strat_pages((args.pubtables, args.doclaynet))
    import lightgbm as lgb
    import numpy as np
    import learned_training_uses_no_rig_document as rig_guard
    rig = {e["document"] for e in rig_guard.rig_entries(HERE.parents[3]) if e.get("document")}
    x, y, groups, docs = [], [], [], []
    for path in (args.pubtables, args.doclaynet):
        pages = collections.defaultdict(list)
        for line in open(path):
            c = json.loads(line)
            if c["document"] in rig:
                raise SystemExit(f"a rig document in the training candidates: {c['document']}")
            pages[c["page"]].append(c)
        per_doc = collections.Counter()
        for page in sorted(pages):
            cs = pages[page]
            if per_doc[cs[0]["document"]] >= args.cap:
                continue
            per_doc[cs[0]["document"]] += 1
            for c in cs:
                x.append(c["x"])
                y.append(int(round(4 * c["grade"])))
            groups.append(len(cs))
            docs.append(cs[0]["document"])
    params = {"objective": "lambdarank", "num_leaves": 31, "max_depth": 6, "learning_rate": 0.05,
              "min_data_in_leaf": 30, "seed": SEED, "deterministic": True, "force_row_wise": True,
              "num_threads": 4, "verbosity": -1, "label_gain": [0, 1, 3, 7, 15]}
    x = np.asarray(x, dtype=np.float32)
    model = lgb.train(params, lgb.Dataset(x, label=np.asarray(y), group=groups, feature_name=NAMES),
                      num_boost_round=args.trees)
    print(f"{len(groups)} pages, {len(x)} candidates, {len(set(docs))} documents", flush=True)
    if args.save:
        model.save_model(args.save)
    if args.model_dir:
        import model_dir
        model_dir.write(args.model_dir, model, x, NAMES, {
            "task": "table_verifier",
            "schema": "pdfluent.features/8",
            "seed": SEED,
            "corpus": "pubtables-1m train (TEDS grades) and doclaynet train (region overlap), re-dumped for #638",
            "documents": [{"document": d, "split": "train"} for d in sorted(set(docs))],
            "rows": {"train": int(len(x))},
            "cap_per_document": args.cap,
            # A RANKER HAS NO THRESHOLD: the judge's floor decides whether a
            # candidate is a table, and this model only orders them.
            "threshold": 0.0,
            "trees": int(model.num_trees()),
            "params": params,
            "rust": "crates/pdf-engine/src/learned/models/table_verifier",
        })
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("extract")
    e.add_argument("--dump", required=True)
    e.add_argument("--set", choices=("pubtables", "doclaynet"), required=True)
    e.add_argument("--truth", default=None)
    e.add_argument("--out", required=True)
    e.add_argument("--jobs", type=int, default=7)
    t = sub.add_parser("train")
    t.add_argument("--pubtables", required=True)
    t.add_argument("--doclaynet", required=True)
    t.add_argument("--cap", type=int, default=10)
    t.add_argument("--trees", type=int, default=300)
    t.add_argument("--save", default=None)
    t.add_argument("--model-dir", default=None)
    args = ap.parse_args()
    return extract(args) if args.cmd == "extract" else train(args)


if __name__ == "__main__":
    raise SystemExit(main())
