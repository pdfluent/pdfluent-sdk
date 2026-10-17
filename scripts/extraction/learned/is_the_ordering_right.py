#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Is the class model's ORDERING right where its scale is wrong? (#602)

    is_the_ordering_right.py <pages-dir-parent> <model.json.gz>

WHY ASK. A model too shy to cross its threshold can still rank: T3 found the
boundary model picks the true cut line as its argmax 51 times of 54 while
almost never crossing its own threshold there, because the class model had
already told it a heading was in that block. A badly calibrated model can be
asked "which of these" when it cannot be asked "is this".

WHAT IT SAYS HERE, on 812 validation pages:

    AUC of the ordering among the blocks the model REFUSES      0.904
    the top-ranked refused paragraph IS the missed heading      41.7 %

So the ordering is strongly informative -- the model is not confused about
these blocks, it is under-confident about all of them -- and it still cannot
be spent. The difference from the boundary lane is the PRIOR, not the model:
there, the prior arrives from another model's decision (this block was
promoted, so a heading is in it); here it would have to be "this page holds a
missed heading", which is the answer.

The one serve-time prior that might substitute is the document's own style,
and that family was measured to cost the heading axis 0.783 -> 0.658, because
a split by document lets a tree recognise the document instead of the block.
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


mine_p, refused_p, per_page = [], [], []
for doc, group in by_doc.items():
    extras = of_document(group)
    for at, page in enumerate(group):
        words = page.get("words") or []
        by_index = {b.get("i"): b for b in page.get("blocks") or []}
        claimed, wanted = set(), set()
        for region in regions_of(page):
            if AS_THE_AXIS_READS_IT.get(region["label"]) != "heading":
                continue
            held = [w for w in words if inside(w, region["bbox"])]
            if not held:
                continue
            owners = {w.get("block") for w in held}
            claimed |= owners
            if len(owners) > 1:
                continue
            block = by_index.get(next(iter(owners)))
            if block is None:
                continue
            text = " ".join(w.get("text") or "" for w in held)
            runs_on = len(block.get("text") or "") > 1.5 * max(1, len(text))
            if OURS.get(block.get("kind"), "paragraph") != "heading" and not runs_on:
                wanted.add(block.get("i"))
        ranked = []
        for block in page.get("blocks") or []:
            if block.get("kind") != "paragraph":
                continue
            e = list(extras.get(((page["page_id"], at), block.get("i")), []))[:len(LOCAL_FEATURES)]
            p = probability(block_vector(block) + e)
            if p >= 0.30:
                continue           # the model already takes these
            ranked.append((p, block.get("i")))
            (mine_p if block.get("i") in wanted else refused_p).append(p)
        # T3'S MOVE IN MY UNITS: on a page that holds a missed heading, is the
        # highest-ranked REFUSED paragraph the missed one?
        if wanted and ranked:
            top = max(ranked)[1]
            per_page.append(top in wanted)

import bisect
def auc(a, b):
    b = sorted(b)
    return sum((bisect.bisect_left(b, x) + bisect.bisect_right(b, x)) / 2
               for x in a) / max(1, len(a) * len(b))

print(f"refused paragraphs that ARE a heading: {len(mine_p)}")
print(f"refused paragraphs that are not:       {len(refused_p)}")
print(f"\nAUC of the model's ordering among the blocks it REFUSES: {auc(mine_p, refused_p):.3f}")
print(f"  (0.5 = the ordering says nothing, 1.0 = perfect)")
print(f"\nT3's move in my units -- on a page holding a missed heading, the")
print(f"highest-ranked refused paragraph IS that heading:")
print(f"  {sum(per_page)} of {len(per_page)} pages  ({100*sum(per_page)/max(1,len(per_page)):.1f}%)")
