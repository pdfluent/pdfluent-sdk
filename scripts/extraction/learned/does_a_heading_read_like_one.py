#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Can WORDS separate the heading the class model keeps calling a paragraph? (#602)

    does_a_heading_read_like_one.py --labels /tmp/pages/labels.jsonl \
        --run /tmp/run-shipped.jsonl

THE CLUSTER THIS IS ABOUT is the largest on the heading axis: a heading that is
already its own block, cut in the right place, and called a paragraph. On 1,200
validation pages it is 276 blocks worth 0.0661, and four attempts at more
GEOMETRY have not moved it -- not a threshold, not sharper features, not the
ordering, not a mirror action.

T3's reading of that, which this measures: no model in the pass reads the TEXT.
Every block feature is geometry, typography or shallow punctuation, and a
heading is a linguistic object before it is a geometric one. If the cut is
already right and the class is still wrong, more geometry cannot separate them
-- which is exactly what four measured noes look like.

SO THIS IS A FALSIFICATION AND NOT A FEATURE. It asks whether trivial lexical
signals separate the cluster from the paragraphs the model correctly refuses,
and reports AUC, which is threshold-free. A signal that cannot separate them
here will not separate them inside a model either, and that is a fourth no
worth as much as a yes.

AND IT IS SPLIT BY DOCUMENT, on T3's caution: a lexical feature is the kind
most likely to recognise the DOCUMENT rather than the block -- the
document-scoped family cost the heading axis 0.783 -> 0.658 doing exactly that
(`document_features.py`). So the AUC is reported per document as well as over
the pool, and a signal that only works pooled is memorising.
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "learned"))
from align_labels import area, overlap, regions_of  # noqa: E402
from worth_of_a_swap import AS_THE_AXIS_READS_IT, read_jsonl  # noqa: E402

OURS = {"paragraph": "paragraph", "heading": "heading", "list_item": "list_item",
        "table_cell": "table", "header": "furniture", "footer": "furniture"}

# A finite verb, by the commonest auxiliaries and copulas. A proxy and not a
# parser: the question is whether the SIGNAL exists, not how well it can be
# measured, and a proxy that finds one settles the question either way.
FINITE = re.compile(
    r"\b(is|are|was|were|be|been|being|has|have|had|do|does|did|can|could|"
    r"may|might|shall|should|will|would|must)\b", re.I)
CLOSING = tuple(".;:!?")

# WHICH OF THESE THE MODEL ALREADY HAS, which decides what a high AUC means.
# `BLOCK_FEATURES` carries `n_words`, `n_chars` and `ends_terminal`, so a
# signal built from those is not news about words -- it is a measurement of
# something the class model reads today and is wrong about anyway. Only a
# signal marked "new" here is evidence for the hypothesis that nothing in the
# pass reads the TEXT (#602).
ALREADY = {
    "no full stop at the end": "ends_terminal",
    "no finite verb": "",
    "under six words": "n_words",
    "words (fewer is more)": "n_words",
    "characters (fewer is more)": "n_chars",
}


def signals(text: str, n_words: float | None = None) -> dict[str, float]:
    """The signals, with the word count THE MODEL RECEIVES and not one of ours.

    `n_words` in `features.rs` is `split_whitespace().count()` -- every token,
    including a punctuation-only one, so `Results -- Discussion` is three words
    to the model. This counted only tokens holding a letter or digit, which is
    two, and then labelled the resulting AUC `n_words` and concluded that the
    model already reads the strongest separator. A different number under the
    feature's name is exactly the mistake this file exists to avoid (codex,
    #602), so the block's own serialized `n_words` is used where the dump
    carries it and the production rule where it does not.
    """
    words = text.split() if n_words is None else [""] * int(n_words)
    stripped = text.strip()
    return {
        # Each is written so that HIGHER means MORE LIKE A HEADING, or the AUC
        # reads inverted and a real signal looks like a broken one.
        "no full stop at the end": 0.0 if stripped.endswith(CLOSING) else 1.0,
        "no finite verb": 0.0 if FINITE.search(text) else 1.0,
        "under six words": 1.0 if len(words) < 6 else 0.0,
        "words (fewer is more)": -float(len(words)),
        "characters (fewer is more)": -float(len(stripped)),
    }


def auc(positive: list[float], negative: list[float]) -> float:
    """The probability that a positive outranks a negative, ties at a half."""
    if not positive or not negative:
        return float("nan")
    values = sorted(positive + negative)
    rank = {}
    i = 0
    while i < len(values):
        j = i
        while j + 1 < len(values) and values[j + 1] == values[i]:
            j += 1
        for k in range(i, j + 1):
            rank[values[k]] = (i + j) / 2.0 + 1.0
        i = j + 1
    total = sum(rank[v] for v in positive)
    n, m = len(positive), len(negative)
    return (total - n * (n + 1) / 2.0) / (n * m)


def page_rows(page: dict) -> tuple[list[str], list[str]]:
    """The cluster's texts on this page, and the paragraphs correctly refused."""
    words = page.get("words") or []
    blocks = page.get("blocks") or []
    by_index = {b.get("i"): b for b in blocks}
    regions = regions_of(page)

    def inside(word, box) -> bool:
        own = area(word["bbox"])
        return own > 0 and overlap(word["bbox"], box) / own > 0.5

    cluster: list[str] = []
    # WHAT THE ANNOTATION CALLS EACH BLOCK OF OURS, by the region holding most
    # of its words. The negatives need this: a paragraph the model is RIGHT
    # about is one the annotation also calls prose, and the first version of
    # this file took the negatives from blocks NO region claimed -- which on
    # 1,200 pages is 134 blocks of leftover junk rather than the tens of
    # thousands of real paragraphs. Every AUC it printed was measured against
    # that (#602).
    said_of: dict[int, str] = {}
    for region in regions:
        held = [w for w in words if inside(w, region["bbox"])]
        if not held:
            continue
        owners = {w.get("block") for w in held}
        for owner in owners:
            if owner is not None:
                share = sum(1 for w in held if w.get("block") == owner)
                best = said_of.get(owner)
                if best is None or share > said_of.get((owner, "n"), 0):
                    said_of[owner] = AS_THE_AXIS_READS_IT.get(region["label"], "")
                    said_of[(owner, "n")] = share
        if AS_THE_AXIS_READS_IT.get(region["label"]) != "heading":
            continue
        if len(owners) > 1:
            continue
        block = by_index.get(next(iter(owners))) or {}
        kind = OURS.get(block.get("kind"), "paragraph")
        text = " ".join(w.get("text") or "" for w in held)
        whole = block.get("text") or ""
        runs_on = len(whole) > 1.5 * max(1, len(text))
        # THE CLUSTER EXACTLY: its own block, not run on, and read as a
        # paragraph. The run-on shape is a different repair and counting it
        # here would price a boundary as a class.
        if kind == "paragraph" and not runs_on:
            cluster.append((whole or text, block.get("n_words")))

    # THE NEGATIVES ARE THE PARAGRAPHS THE MODEL IS RIGHT ABOUT, not every
    # block on the page: the question is whether words tell a heading from a
    # paragraph, and drawing negatives from tables and captions too would
    # answer an easier one. The same bias cost 15x in precision on #602's
    # boundary rule when negatives came only from blocks holding a heading.
    refused: list[str] = []
    for block in blocks:
        if OURS.get(block.get("kind")) != "paragraph":
            continue
        # THE ANNOTATION AGREES IT IS PROSE. Not "no region claims it": that
        # is the empty set of blocks nobody labelled, and it is neither the
        # question nor a fair contrast.
        if said_of.get(block.get("i")) != "paragraph":
            continue
        text = block.get("text") or ""
        if text.strip():
            refused.append((text, block.get("n_words")))
    return cluster, refused


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--labels", type=pathlib.Path, required=True)
    ap.add_argument("--run", type=pathlib.Path, required=True)
    args = ap.parse_args()

    labels = {p["page_id"]: p for p in read_jsonl(args.labels)}
    pool_pos: list[dict] = []
    pool_neg: list[dict] = []
    by_doc: dict[str, tuple[list, list]] = collections.defaultdict(lambda: ([], []))
    for page in read_jsonl(args.run):
        said = labels.get(page["page_id"]) or {}
        page = dict(page, **{k: said[k] for k in ("labels", "label_frame") if k in said})
        cluster, refused = page_rows(page)
        doc = said.get("document") or page.get("page_id")
        for text, n_words in cluster:
            row = signals(text, n_words)
            pool_pos.append(row)
            by_doc[doc][0].append(row)
        for text, n_words in refused:
            row = signals(text, n_words)
            pool_neg.append(row)
            by_doc[doc][1].append(row)

    print(f"the cluster: {len(pool_pos)} block(s); "
          f"paragraphs correctly refused: {len(pool_neg)}")
    if not pool_pos or not pool_neg:
        print("nothing to separate")
        return 1

    names = list(pool_pos[0])
    print(f"\n{'signal':28s}{'AUC pooled':>12s}{'AUC by document':>18s}"
          f"{'docs':>7s}   the model already reads")
    for name in names:
        pooled = auc([r[name] for r in pool_pos], [r[name] for r in pool_neg])
        per = []
        for pos, neg in by_doc.values():
            if pos and neg:
                got = auc([r[name] for r in pos], [r[name] for r in neg])
                if got == got:
                    per.append(got)
        median = statistics.median(per) if per else float("nan")
        has = ALREADY.get(name) or "-- NEW, nothing reads it --"
        print(f"{name:28s}{pooled:12.3f}{median:18.3f}{len(per):7d}   {has}")

    print("\n0.5 is no signal at all; the pooled and the per-document column "
          "disagreeing\nis the signal recognising the DOCUMENT rather than the "
          "block.\n\nAND A HIGH AUC ON A SIGNAL THE MODEL ALREADY READS IS NOT "
          "A LEVER: it is\nseparability the class model has in front of it "
          "today and does not use,\nwhich is a fact about what one feature "
          "can do alone rather than about\nwhat is missing from the vector.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
