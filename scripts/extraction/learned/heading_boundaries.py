#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A heading swallowed by the block under it, and where it could be cut (#602).

THE SIZE, from the join's own report over DocLayNet train
(`alignment/train-all.md`), in REGIONS and not in blocks:

    section_header regions                          118,590
      swallowed by a block of another class          18,813   15.9 %
      merged into one block with another heading      4,063
      split over more than one block of ours          2,661
      no block of ours at all                           415
      -- every loss together                         25,952   21.9 %

The 21.9 per cent that is quoted for this ticket is the WHOLE loss; the
swallowed class on its own is 15.9. Only the swallowed one is a boundary: a
heading merged with another heading, or split across two blocks, is a different
repair and this file does not count it as one.

WHAT A BOUNDARY CAN REACH, which is less again. A heading swallowed at the
FIRST line of the block is not a cut -- there is nothing above it to keep --
and it wants a class, not a boundary. This counts those separately, because a
rule measured over both would be credited for cases it cannot fix.

THE RULE BEING MEASURED (T3's, on #602): a line is a heading if it is short
relative to its COLUMN rather than to the page, ends in no closing
punctuation, and the line above it is finished.

IT DOES NOT WORK, and the best any combination of its clauses reaches is F1
11.2 per cent -- 80.8 recall at 6.0 precision, sixteen lines cut wrongly for
every one cut right. Three things had to be corrected before that number could
be trusted, and each of them made the rule look better than it is:

  * the clauses describe the HEADING, but a boundary opens at line N and for
    the 83.8 per cent of cases where the block BEGINS with the heading, that N
    is the first BODY line -- long, ending in a full stop, nothing like a
    heading. Measured there, a rule about headings was being asked about the
    wrong line;
  * "the line above it is finished" is false BY CONSTRUCTION for those same
    83.8 per cent: a heading that opens its block has no line above it. It is
    not a weak clause, it is a filter that removes the answer. Replaced by
    "nothing above it, or what is above is finished" the rule goes from F1 3.6
    to 85.4;
  * and that 85.4 was measured with negatives drawn only from blocks that
    CONTAIN a swallowed heading -- the population where a cut is most likely to
    be right. Taking the negatives from every block instead, the same rule is
    11.2. The bias was worth 15x in precision.

So the honest reading is that this wants a MODEL and not a rule, the way the
list boundary did (`list_opens`, #599): the same action, the same line
features, and this join as the supervision.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import json
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
import align_labels as al
import table_region as tr

ROOT = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "doclaynet"
CLOSING = tuple(".;:!?,)]}’”")


def read_pages(shards: int, stride: int, split: str = "train"):
    available = sorted((ROOT / "features").glob(f"{split}-*.jsonl.gz"))
    chosen = available[::stride][:shards] if stride > 1 else available[:shards]
    for p in chosen:
        with gzip.open(p, "rt") as f:
            for line in f:
                yield json.loads(line)


def line_labels(page_ids: set, split: str = "train") -> dict:
    """`(page_id, block, line) -> (label, opens)` from T3's join."""
    out = {}
    with open(ROOT / "alignment" / f"{split}-rows.jsonl") as f:
        for text in f:
            row = json.loads(text)
            if row.get("row") != "line" or row["page_id"] not in page_ids:
                continue
            out[(row["page_id"], row["i"], row["line"])] = (row["label"], row["opens"])
    return out


def block_labels(page_ids: set, split: str = "train") -> dict:
    out = {}
    with open(ROOT / "alignment" / f"{split}-rows.jsonl") as f:
        for text in f:
            row = json.loads(text)
            if row.get("row") != "block" or row["page_id"] not in page_ids:
                continue
            out[(row["page_id"], row["i"])] = row["label"]
    return out


def column_of(block: dict, blocks: list) -> tuple[float, float]:
    """The x-range of the COLUMN this block sits in, not of the page.

    Every block whose x-range overlaps this one's by more than half of the
    narrower of the two is taken to be in the same column. A page set in two
    columns then gives a heading a short measure against its own column, which
    is the distinction T3's rule turns on -- a heading spanning a narrow column
    is not short against the page and would be missed.
    """
    mine = (block["bbox"][0], block["bbox"][2])
    lo, hi = mine
    for other in blocks:
        theirs = (other["bbox"][0], other["bbox"][2])
        narrow = min(mine[1] - mine[0], theirs[1] - theirs[0]) or 1.0
        share = (min(mine[1], theirs[1]) - max(mine[0], theirs[0])) / narrow
        if share > 0.5:
            lo = min(lo, theirs[0])
            hi = max(hi, theirs[1])
    return lo, hi


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--shards", type=int, default=12)
    ap.add_argument("--stride", type=int, default=6)
    ap.add_argument("--split", default="train")
    ap.add_argument("--short", type=float, default=0.6,
                    help="a line is short when it fills at most this much of "
                         "its column")
    args = ap.parse_args()

    pages = list(read_pages(args.shards, args.stride, args.split))
    mix = collections.Counter(p.get("category") for p in pages)
    print(f"pages: {len(pages)}")
    for genre, n in mix.most_common():
        print(f"  {genre:<22}{n:>7}  {100*n/len(pages):5.1f}%")
    ids = {p["page_id"] for p in pages}
    lines_label = line_labels(ids, args.split)
    blocks_label = block_labels(ids, args.split)
    print(f"line rows: {len(lines_label)}, block rows: {len(blocks_label)}",
          flush=True)

    shape = collections.Counter()
    cuts = []        # a line where a cut would separate a heading from the rest
    rest = []        # every other interior line: the rule's negatives
    for page in pages:
        pid = page["page_id"]
        words = page.get("words") or []
        blocks = page["blocks"]
        for b in blocks:
            lines = al.lines_of(b, words)
            if len(lines) < 2 or b["kind"] == "heading":
                continue
            marks = [lines_label.get((pid, b["i"], n), (None, False))
                     for n in range(len(lines))]
            heads = [n for n, (label, _) in enumerate(marks)
                     if label == "section_header"]
            opens = [n for n, (label, o) in enumerate(marks)
                     if label == "section_header" and o]
            lo, hi = column_of(b, blocks)
            width = max(1e-6, hi - lo)

            def fact_at(n):
                line = lines[n]
                text = " ".join(w["text"] for w in line).strip()
                x0 = min(w["bbox"][0] for w in line)
                x1 = max(w["bbox"][2] for w in line)
                above = " ".join(w["text"] for w in lines[n - 1]).strip() if n else ""
                return {
                    "short": (x1 - x0) / width <= args.short,
                    "open_end": bool(text) and not text.endswith(CLOSING),
                    "above_finished": bool(above)
                    and above.endswith(tuple(".!?\u2019\u201d")),
                    "at_top": n == 0,
                }

            # WHICH REPAIR THIS BLOCK WANTS, which is not the same question as
            # "is a heading swallowed here". A block that IS the heading wants
            # its class changed; a block that BEGINS with one wants a cut
            # after it; a heading further down wants a cut before it. Counting
            # them together credits a boundary rule for cases only a class can
            # reach, which is how I first counted it.
            # THE NEGATIVES COME FROM EVERY BLOCK, not only from the ones
            # that hold a swallowed heading. Collected after the `continue`
            # below they were, and the rule's precision was then measured
            # only on blocks that contain a heading -- the population where a
            # cut is most likely to be right. That reads as 86.8 per cent and
            # means nothing about a page of plain prose.
            spoken_for = set()
            if opens:
                spoken_for = {len(heads) - 1 if heads == list(range(len(heads)))
                              else opens[0]}
            for n in range(len(lines) - 1):
                if n not in spoken_for:
                    rest.append(fact_at(n))

            if not opens:
                continue
            if len(heads) == len(lines):
                shape["the whole block IS the heading: wants a CLASS"] += 1
                continue
            # THE CLAUSES DESCRIBE THE HEADING, NOT THE LINE THE CUT LANDS
            # ON. A boundary "opens at line N", and where the block begins
            # with a heading that N is the first BODY line -- which is long,
            # ends in a full stop and is nothing like a heading. Measuring
            # T3's rule there makes a rule about headings look useless because
            # it was asked about the wrong line. `at` is the heading; `opens`
            # is where the new block starts.
            if heads and heads == list(range(len(heads))):
                shape["the block BEGINS with the heading: cut after it"] += 1
                cuts.append((fact_at(len(heads) - 1), len(heads)))
            else:
                shape["the heading is further down: cut before it"] += 1
                cuts.append((fact_at(opens[0]), opens[0]))

    print("\nWHAT REPAIR EACH SWALLOWED HEADING WANTS")
    total = sum(shape.values())
    for name, n in shape.most_common():
        print(f"  {name:<46}{n:>7}  {100*n/max(1,total):5.1f}%")
    print(f"  lines a cut would land on:                    {len(cuts):>7}")
    print(f"  interior lines that must NOT be cut:          {len(rest):>7}")
    if not cuts:
        return 0

    cut_facts = [f for f, _ in cuts]

    def rate(rows, name):
        return sum(1 for f in rows if f[name]) / max(1, len(rows))

    print("\n  how often each clause holds, on the cuts and on the rest:")
    for name in ("short", "open_end", "above_finished"):
        print(f"    {name:<16} cuts {100*rate(cut_facts, name):5.1f}%   "
              f"rest {100*rate(rest, name):5.1f}%")

    # EVERY SUBSET, because the rule as proposed scores 3.6 and one of its
    # three clauses is false BY CONSTRUCTION for 84 per cent of the cases: a
    # heading that opens its block has no line above it, so "what is above is
    # finished" can never hold. A clause that cannot be true is not a weak
    # clause, it is a filter that removes the answer.
    names = ("short", "open_end", "above_finished", "nothing_above_or_finished")
    for f in cut_facts + rest:
        f["nothing_above_or_finished"] = f["at_top"] or f["above_finished"]

    print("\n  every combination of clauses, on the cuts and the rest:")
    print(f"    {'clauses':<46}{'recall':>8}{'prec':>8}{'F1':>8}")
    best = None
    for mask in range(1, 1 << len(names)):
        chosen = [names[k] for k in range(len(names)) if mask >> k & 1]
        if "above_finished" in chosen and "nothing_above_or_finished" in chosen:
            continue
        holds = lambda f: all(f[c] for c in chosen)
        hit = sum(1 for f in cut_facts if holds(f))
        false = sum(1 for f in rest if holds(f))
        if not hit:
            continue
        recall = hit / max(1, len(cut_facts))
        precision = hit / (hit + false)
        f1 = 2 * precision * recall / max(1e-9, precision + recall)
        label = " + ".join(chosen)
        print(f"    {label:<46}{100*recall:7.1f}%{100*precision:7.1f}%{100*f1:7.1f}%")
        if best is None or f1 > best[0]:
            best = (f1, label)
    if best:
        print(f"\n  best: {best[1]} at F1 {100*best[0]:.1f}%")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
