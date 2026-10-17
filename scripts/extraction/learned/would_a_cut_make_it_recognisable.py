#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Would a perfect cut make a swallowed heading recognisable? (#602)

    would_a_cut_make_it_recognisable.py <pages-dir-parent> <model.json.gz>

THE QUESTION TWO LANES HAD. The boundary lane repairs headings a block has
swallowed; the class lane repairs blocks that ARE a heading and are called
something else. Both residues sit low with their model, so both looked like
one blindness -- and if they were, a better boundary would never collect the
boundary lane's cluster.

THE TEST. Score the heading PORTION with the class model, as if the cut had
already been made: the block's features with the heading's text, word count,
line count and width in place of the whole block's. If it scores high, that
residue is a boundary problem. If it scores as low as the blocks that are
ALREADY cut out, the class model is blind to those headings either way.

WHAT IT ANSWERED, on 812 validation pages:

    a block that IS the heading            median 0.107   over 0.30:  6.9 %
    heading + body, the whole block        median 0.004   over 0.30: 28.4 %
    the heading portion, as if cut         median 0.063   over 0.30: 34.0 %

A perfect cut buys 5.6 points, not the cluster: two thirds would still be
refused by a cut that could not be improved on. And the blocks that are
already cut out are the HARDER half -- 6.9 per cent against 34.0 -- so the
shared blindness is real and the class lane's residue is its core.

THE CUT IS SYNTHETIC and the number is a floor: a real cut would also change
`gap_below_em` and the neighbour features, both of which point the same way.
Not to be quoted to three figures.
"""
import collections, gzip, json, math, pathlib, sys

sys.path.insert(0, "scripts/extraction/learned")
sys.path.insert(0, "scripts/extraction")
import numpy as np
from features import BLOCK_FEATURES, block_vector
from document_features import LOCAL_FEATURES, of_document
from where_the_heading_axis_loses import OURS
from align_labels import area, overlap, regions_of
from worth_of_a_swap import AS_THE_AXIS_READS_IT

S = pathlib.Path(sys.argv[1])
dump = json.loads(gzip.open(sys.argv[2], "rt").read())
trees, names = dump["tree_info"], dump["feature_names"]
KIND = {"None": 0, "Zero": 1, "NaN": 2}


def margin(x):
    total = 0.0
    for t in trees:
        node = t["tree_structure"]
        while "leaf_value" not in node:
            v = x[node["split_feature"]]
            m = KIND[node["missing_type"]]
            if m != 2 and (v is None or math.isnan(v)):
                v = 0.0
            if (m == 1 and abs(v) <= 1e-35) or (m == 2 and math.isnan(v)):
                left = bool(node["default_left"])
            else:
                left = v <= node["threshold"]
            node = node["left_child"] if left else node["right_child"]
        total += node["leaf_value"]
    return total


def probability(x):
    return 1.0 / (1.0 + math.exp(-margin(x)))


said = {json.loads(l)["page_id"]: json.loads(l) for l in open(S/"pages"/"labels.jsonl")}
pages = []
for line in open(S/"refine-both.jsonl"):
    page = json.loads(line)
    k = said.get(page["page_id"]) or {}
    pages.append(dict(page, **{n: k[n] for n in ("labels", "label_frame") if n in k}))

by_doc = collections.defaultdict(list)
for p in pages:
    by_doc[p.get("document") or p["pdf_sha256"]].append(p)


def inside(word, box):
    own = area(word["bbox"])
    return own > 0 and overlap(word["bbox"], box) / own > 0.5


mine_p, theirs_p, theirs_whole = [], [], []
for doc, group in by_doc.items():
    extras = of_document(group)
    for at, page in enumerate(group):
        words = page.get("words") or []
        by_index = {b.get("i"): b for b in page.get("blocks") or []}
        for region in regions_of(page):
            if AS_THE_AXIS_READS_IT.get(region["label"]) != "heading":
                continue
            held = [w for w in words if inside(w, region["bbox"])]
            if not held:
                continue
            owners = {w.get("block") for w in held}
            if len(owners) > 1:
                continue
            block = by_index.get(next(iter(owners)))
            if block is None:
                continue
            kind = OURS.get(block.get("kind"), "paragraph")
            text = " ".join(w.get("text") or "" for w in held)
            runs_on = len(block.get("text") or "") > 1.5 * max(1, len(text))
            e = list(extras.get(((page["page_id"], at), block.get("i")), []))[:len(LOCAL_FEATURES)]
            if kind != "heading" and not runs_on:
                mine_p.append(probability(block_vector(block) + e))
            elif runs_on:
                # THE HEADING PORTION AS IF IT HAD BEEN CUT OUT. The block's
                # own features with the heading's text and box in place of the
                # whole: what the class model would be asked after a cut.
                cut = dict(block)
                cut["text"] = text
                cut["n_chars"] = len(text)
                cut["n_words"] = len(text.split())
                cut["n_lines"] = 1
                xs = [w["bbox"][0] for w in held] + [w["bbox"][2] for w in held]
                width = (max(xs) - min(xs)) / max(1e-6, page["width"])
                cut["width_frac"] = width
                theirs_p.append(probability(block_vector(cut) + e))
                theirs_whole.append(probability(block_vector(block) + e))

def show(name, v):
    v = sorted(v)
    if not v:
        print(f"  {name}: none"); return
    q = lambda f: v[min(len(v) - 1, int(f * len(v)))]
    over = sum(1 for x in v if x >= 0.30) / len(v)
    print(f"  {name:<34} n={len(v):>4}  median {q(0.5):.3f}  p90 {q(0.9):.3f}  "
          f"over 0.30: {100*over:.1f}%")

print("what the class model says, on 812 validation pages")
show("my 204: block IS the heading", mine_p)
show("T3's: heading + body, whole block", theirs_whole)
show("T3's: heading portion, as if cut", theirs_p)
