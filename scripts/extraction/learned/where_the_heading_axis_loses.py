#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Where the heading axis still loses, and what each shape is worth (#602).

    dump_doclaynet.py --split validation --stride 4 --max-pages 1200 \
        --keep-pages /tmp/pages --out /tmp/val.jsonl
    feature_dump --list /tmp/pages/pages.list --refine list_item,list_opens,heading_lines \
        --out /tmp/on.jsonl
    where_the_heading_axis_loses.py --labels /tmp/pages/labels.jsonl --run /tmp/on.jsonl

THE SWITCHES THE ENGINE SHIPS, all of them: this bins what is left AFTER the
learned pass, and a run with one switch missing counts shapes another switch
already repairs (codex, #1985). `--refine` builds the set from the names it is
given and nothing else.

WORTH IS MEASURED, NOT COUNTED. A cluster's size says how often a shape occurs;
its worth says what the axis would read if that shape were right and nothing
else changed. They are not the same order: a shape that occurs on every page of
one document and nowhere else is large and worth almost nothing.

So each cluster is repaired BY ORACLE on the pages it occurs on -- our heading
list gets the annotation's text where we missed it, or loses the text where we
invented it -- and the axis is scored again. The difference is the cluster's
worth in axis points.
"""
from __future__ import annotations

import argparse
import collections
import pathlib
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "learned"))
import structured_axes as axes  # noqa: E402
from align_labels import CLAIM, area, overlap, regions_of  # noqa: E402
from worth_of_a_swap import (AS_THE_AXIS_READS_IT, f1_of,  # noqa: E402
                             read_jsonl)

OURS = {"paragraph": "paragraph", "heading": "heading", "list_item": "list_item",
        "table_cell": "table", "header": "furniture", "footer": "furniture"}


def page_facts(page: dict) -> dict:
    """Our headings, the annotation's headings, and why each one is missing.

    FROM THE REGION'S SIDE, not the block's. A paragraph that swallowed a
    heading has most of its ink OUTSIDE the heading's region, so asking "which
    blocks does this region claim" (the join's direction, which is about what a
    block should be labelled) puts the commonest shape there is in a bin called
    "no block of ours covers it". The question here is the other one: which of
    our blocks does the region's TEXT end up in.
    """
    words = page.get("words") or []
    blocks = page.get("blocks") or []
    by_index = {block.get("i"): block for block in blocks}
    regions = regions_of(page)

    def inside(word, box) -> bool:
        """Whether this word is in that box, by most of its own area.

        Strictly most: a word exactly half in belongs to neither, which is what
        two regions sharing that edge both need (codex, #1985). The same rule
        as `worth_of_a_swap.annotation_of`, so the two read one truth.
        """
        own = area(word["bbox"])
        return own > 0 and overlap(word["bbox"], box) / own > 0.5

    truth, ours, misses, wrong = [], [], [], []
    heading_blocks: set[int] = set()
    for region in regions:
        if AS_THE_AXIS_READS_IT.get(region["label"]) != "heading":
            continue
        held = [w for w in words if inside(w, region["bbox"])]
        if not held:
            # NOT SCORED, AND NOT A CLUSTER EITHER. The region has none of our
            # glyphs in it: an image of a heading, or text we did not extract.
            # Neither side of the axis has anything to match, and inventing a
            # text for it would price a repair nobody can make here.
            misses.append(("the region holds no text of ours", None, None))
            continue
        text = " ".join(w.get("text") or "" for w in held)
        truth.append(text)
        owners = {w.get("block") for w in held}
        heading_blocks |= owners
        if len(owners) > 1:
            misses.append(("split over more than one block of ours", text, None))
            continue
        block = by_index.get(next(iter(owners))) or {}
        kind = OURS.get(block.get("kind"), "paragraph")
        whole = block.get("text") or ""
        runs_on = len(whole) > 1.5 * max(1, len(text))
        if kind == "heading" and not runs_on:
            continue  # got it
        if kind == "heading":
            # AND WHAT THE REPAIR HAS TO TAKE AWAY. This block is already a
            # heading, with the body hanging off it; adding the right text
            # without removing the wrong one leaves the page with both, so the
            # "repair" buys a false positive and can score WORSE than the
            # shape it was meant to price (codex, #1985).
            misses.append(("a heading block that runs on into its body", text, whole))
        elif runs_on:
            misses.append((f"swallowed by a longer {kind} block", text, None))
        else:
            misses.append((f"a block of its own, read as {kind}", text, None))

    for block in blocks:
        if OURS.get(block.get("kind")) != "heading":
            continue
        text = block.get("text") or ""
        ours.append(text)
        if block.get("i") in heading_blocks:
            continue  # it holds a heading the annotation knows
        held = [w for w in words if w.get("block") == block.get("i")]
        best, share = None, 0.0
        for region in regions:
            if not held:
                break
            got = sum(1 for w in held if inside(w, region["bbox"])) / len(held)
            if got > share:
                best, share = region, got
        if best is None or share < CLAIM:
            wrong.append(("no region at all", text))
        else:
            wrong.append((f"the annotation calls it {best['label']}", text))
    return {"truth": truth, "ours": ours, "misses": misses, "wrong": wrong}


def score(pages: list[dict], repair=None) -> float:
    """The heading axis over these pages, page by page, with one cluster repaired."""
    values = []
    for facts in pages:
        truth, ours = facts["truth"], list(facts["ours"])
        if repair is not None:
            ours = repair(facts, ours)
        got = f1_of(truth, ours)
        if got is not None:
            values.append(got)
    return statistics.fmean(values) if values else float("nan")


def self_test() -> int:
    """A repair has to make the page RIGHT, which is not the same as adding text.

    The shape this is about: a block we already call a heading with the body
    hanging off it. Adding the annotation's text without taking the run-on away
    leaves the page with both, so the repair buys a false positive -- and on a
    page whose only heading is that one it scores WORSE than doing nothing
    (codex, #1985). A worth is a number somebody sizes a ticket with.
    """
    bad = 0

    def check(name, got, want):
        nonlocal bad
        if abs(got - want) > 1e-9:
            bad += 1
            print(f"FAIL {name}: {got:.4f}, wanted {want:.4f}")
        else:
            print(f"ok   {name}")

    facts = {
        "truth": ["Results"],
        "ours": ["Results The study found that the treatment worked"],
        "misses": [("a heading block that runs on into its body", "Results",
                    "Results The study found that the treatment worked")],
        "wrong": [],
    }
    check("the page is wrong before the repair", score([facts]), 0.0)

    def repair(facts, ours):
        mine = [(text, was) for w, text, was in facts["misses"] if text is not None]
        drop = {was for _text, was in mine if was is not None}
        return [t for t in ours if t not in drop] + [text for text, _was in mine]

    check("and right after it", score([facts], repair), 1.0)

    def append_only(facts, ours):
        return ours + [text for _w, text, _was in facts["misses"] if text is not None]

    # Not an assertion about what is right -- a statement of what the other
    # repair would have reported, which is why this test exists.
    print(f"     (appending without replacing would read "
          f"{score([facts], append_only):.4f})")

    print(("FAIL" if bad else "OK") + f": {2 - bad} of 2")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--labels", type=pathlib.Path)
    ap.add_argument("--run", type=pathlib.Path)
    ap.add_argument("--self-test", action="store_true",
                    help="what a repair has to do to count as one; reads no dump")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.labels or not args.run:
        ap.error("--labels and --run, or --self-test")

    labels = {p["page_id"]: p for p in read_jsonl(args.labels)}
    pages = []
    for page in read_jsonl(args.run):
        said = labels.get(page["page_id"]) or {}
        page = dict(page, **{k: said[k] for k in ("labels", "label_frame") if k in said})
        pages.append(page_facts(page))

    base = score(pages)
    print(f"{len(pages)} page(s); the heading axis reads {base:.4f}\n")

    counts: collections.Counter = collections.Counter()
    for facts in pages:
        for why, _text, _was in facts["misses"]:
            counts[("missed", why)] += 1
        for why, _text in facts["wrong"]:
            counts[("invented", why)] += 1

    rows = []
    for (side, why), count in counts.most_common():
        def repair(facts, ours, side=side, why=why):
            if side != "missed":
                return [t for t in ours
                        if t not in {text for w, text in facts["wrong"] if w == why}]
            mine = [(text, was) for w, text, was in facts["misses"]
                    if w == why and text is not None]
            drop = {was for _text, was in mine if was is not None}
            return [t for t in ours if t not in drop] + [text for text, _was in mine]
        rows.append((side, why, count, score(pages, repair) - base))

    print(f"{'cluster':52s}{'n':>6s}{'worth':>9s}")
    for side, why, count, worth in sorted(rows, key=lambda r: -r[3]):
        print(f"{side + ': ' + why:52.52s}{count:6d}{worth:+9.4f}")

    def everything(facts, ours):
        drop = {text for _w, text in facts["wrong"]}
        drop |= {was for _w, _text, was in facts["misses"] if was is not None}
        return [t for t in ours if t not in drop] + \
               [text for _w, text, _was in facts["misses"] if text is not None]
    print(f"\nevery cluster at once: {score(pages, everything) - base:+.4f} "
          f"(the axis would read {score(pages, everything):.4f})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
