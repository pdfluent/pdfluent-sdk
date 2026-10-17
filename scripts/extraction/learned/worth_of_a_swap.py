#!/usr/bin/env python3
"""What a learned class is worth ON THE AXIS, not on blocks (#599).

    worth_of_a_swap.py --features validation-*.jsonl.gz --model blocks-all.txt

WHY THIS EXISTS. A classifier is judged per block: precision, recall and F1
over the rows a join could label. The milestone is judged per AXIS: the share
of a DOCUMENT's annotated headings our markdown holds, matched by text, as
`structured_axes` scores it. Those are two questions and they move apart -- a
model can win a hundredth of block F1 on blocks the axis never looks at, and
lose the axis by moving one heading a reader would have noticed.

So the axis is scored twice over the same pages: once with the classes the
engine emits, once with the model's class in their place, and the difference is
printed. Only the DIFFERENCE means anything here. The absolute figure is not
the round's, because the round matches against the annotation's own text and
this matches against the text of the blocks each region claims -- the join.
What both sides share is the block boundaries, which is what makes the two
columns comparable: a class swap moves classes and nothing else.

ON VALIDATION, NEVER ON THE RIG. DocLayNet's validation split is neither the
rig's tuning half nor its sealed fifths: the model never trained on it and no
axis is pinned to it, so the question is answered without the measurement set
being on this machine at all. The round measures the rig.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import json
import pathlib
import statistics
import sys

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import structured_axes as axes  # noqa: E402
from align_labels import area, ink_of, overlap, regions_of  # noqa: E402
from document_features import LOCAL_FEATURES, of_document  # noqa: E402
from features import block_vector  # noqa: E402
from train_blocks import CLASSES  # noqa: E402

# DocLayNet's eleven as the axes read them, which is `structured_bench`'s own
# mapping of the annotation.
AS_THE_AXIS_READS_IT = {
    "caption": "paragraph", "footnote": "paragraph", "formula": "paragraph",
    "list_item": "list_item", "page_footer": "furniture",
    "page_header": "furniture", "picture": "ignore",
    "section_header": "heading", "table": "table", "text": "paragraph",
    "title": "heading",
}
# And our own kinds, as the same axes read them.
OURS_AS_THE_AXIS_READS_IT = {
    "paragraph": "paragraph", "heading": "heading", "list_item": "list_item",
    "table_cell": "table", "header": "furniture", "footer": "furniture",
}
AXIS_CLASSES = ("heading", "list_item", "table", "paragraph")


def read_jsonl(path: pathlib.Path) -> list[dict]:
    if path.suffix == ".gz":
        with gzip.open(path, "rt") as lines:
            return [json.loads(line) for line in lines if line.strip()]
    return [json.loads(line) for line in path.read_text().split("\n") if line.strip()]


def f1_of(truth: list[str], produced: list[str]) -> float | None:
    """One class of one document, scored the way the round scores it."""
    truth = [t for t in (axes.normalise(t) for t in truth) if t]
    produced = [p for p in (axes.normalise(p) for p in produced) if p]
    if not truth:
        return None
    if not produced:
        return 0.0
    pairs = axes.match_items(truth, produced)
    precision, recall = len(pairs) / len(produced), len(pairs) / len(truth)
    return 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)


def annotation_of(pages: list[dict]) -> dict[str, list[str]]:  # noqa: D401
    """What each region says, taken from OUR WORDS INSIDE IT.

    The dump carries the annotation's BOXES and our words' text and positions,
    so a region's text is the text of the words that fall in it. That is the
    nearest thing to the annotation's own transcription that a dump holds, and
    it is available whatever our blocks did with the region.

    FROM THE REGION'S SIDE, AND THAT IS THE WHOLE POINT. This was written the
    other way round -- a region's text was the text of the BLOCKS whose ink is
    mostly inside it -- and a paragraph that swallowed a heading has most of
    its ink OUTSIDE the heading's region, so it claimed nothing and the region
    was dropped from the truth entirely. Swallowed headings are the largest
    shape there is (498 of 1 200 validation pages, worth 0.125 on the heading
    axis), so the instrument was blind to the cases the work is about, and any
    repair that recovered one was scored as an invention.

    It cost a real decision: measured this way, `heading_ends` read -0.016 and
    shipped OFF; measured from the region's side it reads +0.033 and repairs
    149 of 494 swallowed headings while breaking none (#602, #1983).

    A region with none of our words in it is still missing from both columns --
    an image of a heading, or text we did not extract. Nothing in a dump can
    price that, and inventing a text for it would price a repair nobody can
    make here.
    """
    out: dict[str, list[str]] = collections.defaultdict(list)
    for page in pages:
        words = page.get("words") or []
        for region in regions_of(page):
            reads_as = AS_THE_AXIS_READS_IT.get(region["label"])
            if reads_as not in AXIS_CLASSES:
                continue
            box = region["bbox"]
            # BY MOST OF THE WORD'S OWN AREA, strictly. A word exactly half in
            # and half out belongs to neither, which is what the two regions
            # sharing that edge both need; `>= 0.5` gave it to both of them
            # (codex, #1985). A box edge drawn through a line of text would
            # otherwise take every word it grazes.
            held = [word.get("text") or "" for word in words
                    if area(word["bbox"]) > 0
                    and overlap(word["bbox"], box) / area(word["bbox"]) > 0.5]
            if held:
                out[reads_as].append(" ".join(held))
    return out


def self_test() -> int:
    """The truth this file builds, on the shape it was blind to.

    A swallowed heading -- the annotation's region inside one of our longer
    blocks -- must be IN the truth, because it is what a repair recovers. Built
    from the blocks that claim a region it was not, and the tools that read this
    scored every such repair as an invention (#602, #1983).
    """
    bad = 0

    def check(name, got, want):
        nonlocal bad
        if got != want:
            bad += 1
            print(f"FAIL {name}: {got!r}, wanted {want!r}")
        else:
            print(f"ok   {name}")

    def word(text, x, y):
        return {"text": text, "bbox": [x, y, x + 10, y + 8], "block": 0}

    # ONE BLOCK of ours holding two lines: a heading over its body. The
    # annotation marks only the heading, whose region covers the top line.
    page = {
        "page_id": "p", "width": 100, "height": 100,
        "label_frame": {"coco_width": 100, "coco_height": 100},
        "labels": [{"category_id": 8, "bbox": [8, 8, 30, 10]}],
        "words": [word("Results", 10, 82), word("The", 10, 70), word("study", 22, 70)],
        "blocks": [{"i": 0, "kind": "paragraph", "text": "Results The study",
                    "bbox": [10, 70, 60, 90]}],
    }
    truth = annotation_of([page])
    check("a swallowed heading is in the truth", truth.get("heading"), ["Results"])
    check("and the block that swallowed it is not", truth.get("paragraph"), None)

    # A region with none of our words in it stays out of both columns: nothing
    # in a dump can price a repair to it.
    empty = dict(page, words=[], blocks=[])
    check("a region with no words of ours is left out",
          annotation_of([empty]).get("heading"), None)

    print(("FAIL" if bad else "OK") + f": {3 - bad} of 3")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--features", nargs="+", type=pathlib.Path)
    ap.add_argument("--model", type=pathlib.Path)
    ap.add_argument("--self-test", action="store_true",
                    help="the truth this file builds, on the shape it was "
                         "blind to; reads no dump")
    ap.add_argument("--klass", default="section_header",
                    help="the ONE class the model may set and take away, the "
                         "way `learned::refine` switches a class per lane; "
                         "`all` lets it decide every block, which is what a "
                         "block F1 measures and not what a lane would land")
    ap.add_argument("--body-on-removal", action="store_true",
                    help="a block the model takes its class away from becomes "
                         "body text, rather than whatever the model's second "
                         "opinion says: a lane owns one class")
    ap.add_argument("--least", type=float, default=0.5,
                    help="the confidence under which the model says nothing and "
                         "our own class stands")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.features or not args.model:
        ap.error("--features and --model, or --self-test")

    import lightgbm as lgb
    import numpy as np
    model = lgb.Booster(model_file=str(args.model))
    # A BINARY MODEL IS WHAT A LANE LANDS: one class against the rest, which is
    # what `learned::refine` switches and what the exporter and the size budget
    # of #598 take. A multiclass model is read here too, because it is the
    # instrument that compares classes -- but it is not what ships.
    binary = model.num_model_per_iteration() == 1

    documents: dict[str, list[dict]] = collections.defaultdict(list)
    for path in args.features:
        for page in read_jsonl(path):
            documents[page.get("document") or page["page_id"]].append(page)

    before = collections.defaultdict(list)
    after = collections.defaultdict(list)
    moved: collections.Counter = collections.Counter()
    for pages in documents.values():
        rows = [((page["page_id"], at_page), block)
                for at_page, page in enumerate(pages)
                for block in (page.get("blocks") or [])]
        if not rows:
            continue
        of_the_document = of_document(pages)
        matrix = np.asarray(
            [block_vector(block)
             + list(of_the_document[(page_id, block.get("i"))][:len(LOCAL_FEATURES)])
             for page_id, block in rows], dtype="float32")
        said = model.predict(matrix)
        if binary:
            said = np.column_stack([1.0 - said, said])
        was_of: list[str] = []
        now_of: list[str] = []
        mine = (1 if binary else
                None if args.klass == "all" else CLASSES.index(args.klass))
        reads_as = None if mine is None else AS_THE_AXIS_READS_IT[args.klass]
        for (_page_id, block), probabilities in zip(rows, said):
            text = block.get("text") or ""
            was = OURS_AS_THE_AXIS_READS_IT.get(block.get("kind"), "paragraph")
            best = int(probabilities.argmax())
            if mine is None:
                # Every class at once: what a block F1 measures, and not what
                # any lane would switch on.
                now = (was if probabilities[best] < args.least
                       else AS_THE_AXIS_READS_IT[CLASSES[best]])
            else:
                # ONE CLASS, SET AND TAKEN AWAY, which is what `learned::refine`
                # switches per lane: this model may make a block a heading and
                # may take the heading away from one, and may say nothing about
                # any other class. A model let loose on every class moves
                # thousands of blocks between classes nobody asked it about --
                # on one validation shard, 3 951 paragraphs into tables and
                # 3 174 tables back out.
                says_mine = probabilities[mine] >= args.least
                if says_mine:
                    now = reads_as
                elif was == reads_as:
                    # TAKING A CLASS AWAY PUTS THE BLOCK IN THE BODY, and not
                    # wherever this model's second opinion points. A lane owns
                    # one class; moving a block it rejected into `table` or
                    # `furniture` is a decision about classes nobody switched
                    # on, and on one shard that was 335 headings into tables
                    # and 164 into furniture.
                    now = "paragraph" if (args.body_on_removal or binary) else (
                        AS_THE_AXIS_READS_IT[CLASSES[best]]
                        if probabilities[best] >= args.least else "paragraph")
                else:
                    now = was
            was_of.append(was)
            now_of.append(now)
            if now != was:
                moved[f"{was} -> {now}"] += 1
        for at_this_page, page in enumerate(pages):
            # PER PAGE, AND THAT IS A CHOICE. The axis the round reports is per
            # document, and matching a document's thousands of blocks against
            # each other is the greedy pairing of `match_items` over a list of
            # that length -- hours for one shard. A page is tens of blocks and
            # seconds for all of them. What is compared here is two columns
            # over the SAME pages with the same boundaries, so the difference
            # is the swap's; the absolute figure is neither the round's nor a
            # document's.
            annotated = annotation_of([page])
            mine_now, mine_was = collections.defaultdict(list), collections.defaultdict(list)
            for (page_id, block), was, now in zip(rows, was_of, now_of):
                if page_id[0] != page["page_id"] or page_id[1] != at_this_page:
                    continue
                text = block.get("text") or ""
                if was in AXIS_CLASSES:
                    mine_was[was].append(text)
                if now in AXIS_CLASSES:
                    mine_now[now].append(text)
            for name in AXIS_CLASSES:
                truth = annotated.get(name) or []
                was_f1 = f1_of(truth, mine_was.get(name) or [])
                now_f1 = f1_of(truth, mine_now.get(name) or [])
                if was_f1 is not None:
                    before[name].append(was_f1)
                    after[name].append(now_f1)

    print(f"{len(documents)} document(s) of the validation split\n")
    print(f"{'axis':12s}{'documents':>11s}{'ours':>9s}{'the model':>11s}{'worth':>9s}")
    for name in AXIS_CLASSES:
        if not before[name]:
            continue
        was = statistics.fmean(before[name])
        now = statistics.fmean(after[name])
        print(f"{name:12s}{len(before[name]):11d}{was:9.4f}{now:11.4f}{now - was:+9.4f}")
    print("\nwhat the model moved, and how often:")
    for what, count in moved.most_common(10):
        print(f"  {count:7d}  {what}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
