#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A small graph network over a page's lines: what each line is, and where a
region ends (#638, #633).

    ~/.cache/pdfluent/gnn-venv/bin/python scripts/extraction/learned/train_line_gnn.py \
        --features ~/Datasets/pdfluent-training/doclaynet/features \
        --rows ~/Datasets/pdfluent-training/doclaynet/alignment/train-rows-ordered.jsonl \
        --out /tmp/line-gnn

THE CONCEPT, and only the concept, is pymupdf4llm's text-only layout model
(#633): lines are nodes, edges join a line to its nearest neighbours in the
plane, and two heads read the result -- a class per line, and whether a line
belongs to the same region as the next one in reading order. No code or
weights are taken from anywhere.

EVERY INPUT IS ONE THE ENGINE ALREADY COMPUTES, so the Rust side is a
matrix product and a gather, not a second reading of the page:
- per line: `LINE_FEATURES` (23), its block's vector (30), where it stands
  against the page's figures (4, `learned::captions`), and its box as
  fractions of the page (4);
- per edge: the gap between the two boxes in the source line's ems, the
  difference of their left edges, their size ratio, and whether the heuristics
  put them in one block.

WHAT IS LEARNT. Classes: other, heading, list, table, caption, page_header,
page_footer (the heading axis folds section_header and title; DocLayNet's own labels, by the join).
Links: between each line and the next in reading order, whether the join put
them in one region. A line with no region is left out of both losses.

THE SPLIT, THE CAP AND THE GUARDS are the lanes' own: `train_lists.third` by
document, at most `--pages-per-document` pages of any document, the train
split only, no dump made with the learned pass on and no dump another engine
tree segmented (`training_rules`).
"""
from __future__ import annotations

import argparse
import collections
import gzip
import hashlib
import json
import math
import pathlib
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import training_rules  # noqa: E402
from align_labels import lines_of  # noqa: E402
from document_features import LOCAL_FEATURES, of_document  # noqa: E402
from features import block_vector  # noqa: E402
from list_lines import _box, _size, heads_on_page, line_vectors, prev_colon  # noqa: E402
from train_heading_lines import chosen_pages, picture_features  # noqa: E402
from train_lists import SEED, third  # noqa: E402
from style import STYLE_FEATURES, page_style  # noqa: E402

CLASSES = ["other", "heading", "list", "table", "caption", "page_header", "page_footer"]
CLASS_OF = {"section_header": 1, "title": 1, "list_item": 2, "table": 3, "caption": 4,
            "page_header": 5, "page_footer": 6}
# Where `repeats_in_document` sits in a node row: after the line's own
# features, at its place in the block vector. A loose page reads 1 there.
REPEATS = 23 + 23
# Directions a line looks for its nearest neighbour in, then its neighbours in
# reading order. One edge type each, so the network can weigh "the line above"
# apart from "the line to the right".
DIRECTIONS = ["above", "below", "left", "right", "prev", "next"]
# And what the line is set in against the page's body (`style.py`, #574):
# a heading on a financial report is the body's size in a heavier face or
# another colour more often than it is larger.
BASE_WIDTH = 23 + 30 + 4 + 4
# THE GROUPS A NODE MAY CARRY past the base, in this order (#574). The graph
# cache always holds all of them; `--groups` picks which the network reads, so
# one cache serves every arm of an ablation, and the export writes the chosen
# list so the engine builds the same row.
#   style  -- what the line is set in against the page's body (`style.py`);
#   vector -- what the page draws round the line (T5, `drawings.py`,
#             the dump's `line_vector`);
#   judged -- the line's block is a table cell the table judge did not choose:
#             prose the builder drew a grid over, which the heading decision
#             reads before the table pass demotes it.
GROUP_WIDTH = {"style": len(STYLE_FEATURES), "vector": 5, "judged": 1}
GROUP_ORDER = ["style", "vector", "judged"]
FULL_WIDTH = BASE_WIDTH + sum(GROUP_WIDTH.values())
NODE_WIDTH = FULL_WIDTH


def group_columns(groups: list[str]) -> list[int]:
    """The columns of a full row the network reads with `groups` on."""
    cols, at = list(range(BASE_WIDTH)), BASE_WIDTH
    for g in GROUP_ORDER:
        if g in groups:
            cols += list(range(at, at + GROUP_WIDTH[g]))
        at += GROUP_WIDTH[g]
    return cols
EDGE_WIDTH = 4


def page_graph(record: dict, facts: dict | None, at_page: int):
    """Nodes, edges and the order of the page's lines, as the engine would
    build them: blocks in the page's order, lines in `lines_of`'s."""
    words = record.get("words") or []
    blocks = record.get("blocks") or []
    heads = heads_on_page(blocks, words)
    width = record.get("width") or 1.0
    height = record.get("height") or 1.0
    images = record.get("images") or []
    nodes, meta = [], []
    styles = page_style(record)
    vectors_of_page = record.get("line_vector") or []
    first = 0
    for at, block in enumerate(blocks):
        pieces = lines_of(block, words)
        vectors = line_vectors(block, words, record.get("body_size") or 1.0, heads,
                               prev_colon(blocks, at))
        extra = (facts or {}).get(((record.get("page_id"), at_page), block.get("i")), [])
        extra = list(extra[:len(LOCAL_FEATURES)])
        whole = block_vector(block) + extra + [math.nan] * (len(LOCAL_FEATURES) - len(extra))
        unchosen = 1.0 if block.get("kind") == "table_cell" and block.get("judged") != "table" else 0.0
        for k, (piece, v) in enumerate(zip(pieces, vectors)):
            box, size = _box(piece), _size(piece)
            drawn = (list(vectors_of_page[first + k]) if first + k < len(vectors_of_page)
                     else [math.nan] * GROUP_WIDTH["vector"])
            nodes.append(list(v) + whole + picture_features(box, size, images)
                         + [box[0] / width, box[1] / height, box[2] / width, box[3] / height]
                         + list(styles[first + k]) + drawn + [unchosen])
            meta.append((block["i"], k, box, size))
        first += len(pieces)
    return nodes, meta, edges_of(meta)


def edges_of(meta):
    """Per direction, the neighbour of every line: nearest above/below among
    lines that overlap it across, nearest left/right among lines that overlap
    it down, and the previous/next line in reading order. -1 where none."""
    n = len(meta)
    out = {d: [-1] * n for d in DIRECTIONS}
    for i, (_b, _k, (x0, y0, x1, y1), _s) in enumerate(meta):
        best = {d: (math.inf, -1) for d in ("above", "below", "left", "right")}
        for j, (_b2, _k2, (a0, b0, a1, b1), _s2) in enumerate(meta):
            if i == j:
                continue
            across = min(x1, a1) - max(x0, a0) > 0
            down = min(y1, b1) - max(y0, b0) > 0
            if across and b0 >= y1 and b0 - y1 < best["above"][0]:
                best["above"] = (b0 - y1, j)
            if across and y0 >= b1 and y0 - b1 < best["below"][0]:
                best["below"] = (y0 - b1, j)
            if down and x0 >= a1 and x0 - a1 < best["left"][0]:
                best["left"] = (x0 - a1, j)
            if down and a0 >= x1 and a0 - x1 < best["right"][0]:
                best["right"] = (a0 - x1, j)
        for d in best:
            out[d][i] = best[d][1]
        out["prev"][i] = i - 1 if i > 0 else -1
        out["next"][i] = i + 1 if i + 1 < n else -1
    return out


def edge_features(meta, i, j):
    """What an edge carries: the gap in the source line's ems, the difference
    of the left edges in ems, the size ratio, and one block or two."""
    if j < 0:
        return [math.nan] * EDGE_WIDTH
    bi, _ki, (x0, y0, x1, y1), si = meta[i]
    bj, _kj, (a0, b0, a1, b1), sj = meta[j]
    em = si if si > 0 else 1.0
    gap = math.hypot(max(a0 - x1, x0 - a1, 0.0), max(b0 - y1, y0 - b1, 0.0)) / em
    return [gap, (a0 - x0) / em, (sj or 1.0) / em, 1.0 if bi == bj else 0.0]


def load(args):
    """Every chosen page as (x, edges, edge features, classes, links, part)."""
    import numpy as np
    wanted = chosen_pages(args.features, args.pages_per_document, None,
                           args.engine_skew_is_fine)
    print(f"[gnn] {len(wanted)} page(s) chosen", flush=True)
    labels = collections.defaultdict(dict)
    with args.rows.open(encoding="utf-8") as f:
        for text in f:
            if '"row": "line"' not in text:
                continue
            r = json.loads(text)
            if r.get("region") is None or r["page_id"] not in wanted:
                continue
            labels[r["page_id"]][(r["i"], r["line"])] = (r["label"], r["region"])
    graphs = []

    def take(document, pages):
        facts = of_document(pages)
        for at_page, r in enumerate(pages):
            if r["page_id"] not in wanted or "images" not in r:
                continue
            # A STYLE ROW NEEDS EVERY WORD'S FACE: a word the merge could not
            # place would read as "another face than the body", which the
            # engine never produces, and heading lines carry such a word 4.6
            # times as often as others (#574).
            if not all("font" in w for w in r.get("words") or []):
                continue
            said = labels.get(r["page_id"], {})
            nodes, meta, edges = page_graph(r, facts, at_page)
            if len(nodes) < 2:
                continue
            cls = [CLASS_OF.get(said[(b, k)][0], 0) if (b, k) in said else -1
                   for b, k, _box, _s in meta]
            link = []
            for i in range(len(meta) - 1):
                a, c = said.get(meta[i][:2]), said.get(meta[i + 1][:2])
                link.append(-1 if a is None or c is None else int(a[1] == c[1]))
            link.append(-1)
            idx = np.array([[edges[d][i] for d in DIRECTIONS] for i in range(len(meta))], dtype=np.int64)
            ef = np.array([[f for d in DIRECTIONS for f in edge_features(meta, i, edges[d][i])]
                           for i in range(len(meta))], dtype=np.float32)
            graphs.append((np.nan_to_num(np.array(nodes, dtype=np.float32), nan=-1.0), idx,
                           np.nan_to_num(ef, nan=-1.0), np.array(cls), np.array(link),
                           third(document), document))

    # A DOCUMENT AT A TIME, as the other trainers read the dump: holding every
    # shard would not fit this Mac.
    current, held = None, []
    for path in sorted(args.features.glob("train-*.jsonl.gz")):
        with gzip.open(path, "rt", encoding="utf-8") as f:
            for text in f:
                r = json.loads(text)
                training_rules.only_the_train_split(r, path.name, None)
                training_rules.not_its_own_output(r, path.name, None)
                training_rules.same_engine(r, path.name, args.engine_skew_is_fine)
                document = r.get("document") or r["pdf_sha256"]
                if document != current and held:
                    take(current, held)
                    held = []
                current = document
                held.append(r)
    if held:
        take(current, held)
    return graphs


def model(hidden: int, layers: int, width: int | None = None):
    import torch
    import torch.nn as nn

    class LineGnn(nn.Module):
        """Encode each line; `layers` rounds of: each line reads its six
        neighbours (their state and the edge between), one weight per
        direction, and adds it to its own; then a class head per line and a
        link head on (line, next line)."""

        def __init__(self):
            super().__init__()
            self.encode = nn.Linear(width or NODE_WIDTH, hidden)
            self.edge = nn.ModuleList(nn.Linear(hidden + EDGE_WIDTH, hidden) for _ in DIRECTIONS)
            self.own = nn.ModuleList(nn.Linear(hidden, hidden) for _ in range(layers))
            self.mix = nn.ModuleList(nn.Linear(hidden * len(DIRECTIONS), hidden) for _ in range(layers))
            self.classes = nn.Linear(hidden, len(CLASSES))
            self.link = nn.Linear(2 * hidden + EDGE_WIDTH, 1)
            self.layers = layers

        def forward(self, x, idx, ef):
            h = torch.relu(self.encode(x))
            n = h.shape[0]
            pad = torch.zeros(1, h.shape[1])
            for layer in range(self.layers):
                hp = torch.cat([h, pad], 0)
                msgs = []
                for d in range(len(DIRECTIONS)):
                    j = torch.where(idx[:, d] < 0, torch.full_like(idx[:, d], n), idx[:, d])
                    e = ef[:, d * EDGE_WIDTH:(d + 1) * EDGE_WIDTH]
                    msgs.append(torch.relu(self.edge[d](torch.cat([hp[j], e], 1))))
                h = torch.relu(self.own[layer](h) + self.mix[layer](torch.cat(msgs, 1)))
            nxt = DIRECTIONS.index("next")
            j = torch.where(idx[:, nxt] < 0, torch.full_like(idx[:, nxt], n), idx[:, nxt])
            hp = torch.cat([h, pad], 0)
            link = self.link(torch.cat([h, hp[j], ef[:, nxt * EDGE_WIDTH:(nxt + 1) * EDGE_WIDTH]], 1))
            return self.classes(h), link.squeeze(1)

    return LineGnn()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--features", type=pathlib.Path, required=True)
    ap.add_argument("--rows", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--pages-per-document", type=int, default=20)
    ap.add_argument("--hidden", type=int, default=64)
    ap.add_argument("--layers", type=int, default=3)
    ap.add_argument("--epochs", type=int, default=8)
    ap.add_argument("--engine-skew-is-fine", default=None, metavar="REASON",
                    help="train on a dump another engine tree segmented, and "
                         "say why; without it such a dump is refused")
    ap.add_argument("--single-page-share", type=float, default=0.0,
                    help="share of training pages read as a loose page: "
                         "`repeats_in_document` set to 1, what a one-page PDF gives")
    ap.add_argument("--groups", default="",
                    help="comma list of " + ", ".join(GROUP_ORDER) + " the network reads "
                         "past the base (#574); empty is the base alone")
    ap.add_argument("--cache", type=pathlib.Path, default=None,
                    help="the graphs of every arm, built once (all groups)")
    ap.add_argument("--five-classes", action="store_true",
                    help="fold page_header and page_footer into other: the classes "
                         "the shipped network has, to tell the new classes from the new data")
    args = ap.parse_args()
    groups = [g for g in args.groups.split(",") if g]
    global CLASSES
    unknown = set(groups) - set(GROUP_ORDER)
    if unknown:
        ap.error(f"unknown group(s) {sorted(unknown)}")
    import numpy as np
    import torch
    torch.manual_seed(SEED)
    t = time.time()
    cache = args.cache or args.out / "graphs.pkl"
    if cache.exists():
        import pickle
        graphs = pickle.load(open(cache, "rb"))
    else:
        graphs = load(args)
        import pickle
        args.out.mkdir(parents=True, exist_ok=True)
        pickle.dump(graphs, open(cache, "wb"))
    print(f"[gnn] {len(graphs)} pages ({time.time() - t:.0f}s)", flush=True)
    cols = group_columns(groups)
    graphs = [(g[0][:, cols],) + tuple(g[1:]) for g in graphs]
    if args.five_classes:
        CLASSES = CLASSES[:5]
        graphs = [g[:3] + (np.where(g[3] >= 5, 0, g[3]),) + tuple(g[4:]) for g in graphs]
    print(f"[gnn] groups {groups or ['base']}: {len(cols)} node inputs", flush=True)
    args.out.mkdir(parents=True, exist_ok=True)
    # STANDARDISED INPUTS: the features run from fractions to character
    # counts, and a network fed them raw learns the large ones first. The mean
    # and spread are the training pages', stored beside the weights so the
    # engine applies the same.
    # A LOOSE PAGE HAS NO DOCUMENT to repeat its running head in, and a
    # network that learnt furniture from repetition alone would not see it
    # there (#574). So a share of the training pages is read as the engine
    # reads a one-page PDF; the validation pages are left as they are.
    loose = np.random.default_rng(SEED + 1)
    for g in graphs:
        if g[5] == "train" and loose.random() < args.single_page_share:
            g[0][:, REPEATS] = 1.0
    allx = np.concatenate([g[0] for g in graphs if g[5] == "train"])
    alle = np.concatenate([g[2] for g in graphs if g[5] == "train"])
    mx, sx = allx.mean(0), allx.std(0) + 1e-6
    me, se = alle.mean(0), alle.std(0) + 1e-6
    graphs = [((g[0] - mx) / sx, g[1], (g[2] - me) / se) + tuple(g[3:]) for g in graphs]
    np.savez(args.out / "norm.npz", mx=mx, sx=sx, me=me, se=se)
    tr = [g for g in graphs if g[5] == "train"]
    va = [g for g in graphs if g[5] == "validation"]
    net = model(args.hidden, args.layers, len(cols))
    print(f"[gnn] {sum(p.numel() for p in net.parameters())} parameters", flush=True)
    opt = torch.optim.Adam(net.parameters(), lr=1e-3)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=args.epochs)
    counts = np.bincount(np.concatenate([g[3][g[3] >= 0] for g in tr]), minlength=len(CLASSES))
    weight = torch.tensor((counts.sum() / np.maximum(counts, 1)) ** 0.5, dtype=torch.float32)
    ce = torch.nn.CrossEntropyLoss(weight=weight / weight.mean(), ignore_index=-1)
    bce = torch.nn.BCEWithLogitsLoss()
    tensors = lambda g: (torch.from_numpy(g[0]), torch.from_numpy(g[1]), torch.from_numpy(g[2]),
                         torch.from_numpy(g[3]), torch.from_numpy(g[4]))

    def evaluate(split):
        net.eval()
        tp = collections.Counter(); said = collections.Counter(); truth = collections.Counter()
        lk = [0, 0]
        with torch.no_grad():
            for g in split:
                x, idx, ef, y, link = tensors(g)
                logits, lg = net(x, idx, ef)
                pred = logits.argmax(1)
                m = y >= 0
                for c in range(len(CLASSES)):
                    tp[c] += int(((pred == c) & (y == c) & m).sum()); said[c] += int(((pred == c) & m).sum())
                    truth[c] += int(((y == c) & m).sum())
                lm = link >= 0
                lk[0] += int(((lg > 0).long() == link)[lm].sum()); lk[1] += int(lm.sum())
        f1 = {CLASSES[c]: round(2 * tp[c] / max(1, said[c] + truth[c]), 4) for c in range(len(CLASSES))}
        net.train()
        return f1, round(lk[0] / max(1, lk[1]), 4)

    rng = np.random.default_rng(SEED)
    for epoch in range(args.epochs):
        order = rng.permutation(len(tr))
        total = 0.0
        for at in order:
            x, idx, ef, y, link = tensors(tr[at])
            logits, lg = net(x, idx, ef)
            lm = link >= 0
            # A PAGE WITH NOTHING LABELLED teaches nothing, and an empty
            # cross-entropy is NaN, which poisons every weight it touches.
            if not (y >= 0).any() and not lm.any():
                continue
            loss = (ce(logits, y) if (y >= 0).any() else torch.zeros(()))
            loss = loss + (bce(lg[lm], link[lm].float()) if lm.any() else torch.zeros(()))
            opt.zero_grad(); loss.backward(); opt.step()
            total += float(loss)
        sched.step()
        f1, acc = evaluate(va)
        print(f"[gnn] epoch {epoch + 1}: loss {total / len(tr):.4f}  val class F1 {f1}  link acc {acc}", flush=True)
    torch.save(net.state_dict(), args.out / "line_gnn.pt")
    # This lane writes no manifest; the skew the other lanes record there goes
    # beside the shape the weights are read with (#574).
    config = {"hidden": args.hidden, "layers": args.layers, "classes": CLASSES,
              "directions": DIRECTIONS, "node_width": len(cols),
              "edge_width": EDGE_WIDTH, "groups": groups}
    skew = training_rules.engine_skew_manifest()
    if skew is not None:
        config["engine_skew"] = skew
    json.dump(config, open(args.out / "config.json", "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
